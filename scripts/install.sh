#!/usr/bin/env bash
# Build Veil App from source: installs Rust and Bun when missing, then runs
# `bun run tauri build`. Usage: ./scripts/install.sh
set -euo pipefail

# Minimum Rust version, kept in sync with `rust-version` in src-tauri/Cargo.toml.
MIN_RUST="1.90.0"

info() { printf '\033[1;34m==>\033[0m %s\n' "$1"; }
fail() { printf '\033[1;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

# Run from the repository root regardless of where the script is invoked.
cd "$(dirname "$0")/.."

[[ "$(uname -s)" == "Darwin" ]] || fail "Veil App is a macOS app; run this on macOS."
command -v curl >/dev/null || fail "curl is required."

# --- Rust -------------------------------------------------------------------
# A previous rustup install may not be on PATH in this shell yet.
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"

if command -v cargo >/dev/null && command -v rustc >/dev/null; then
  info "Rust found: $(rustc --version)"
  current="$(rustc --version | awk '{print $2}')"
  if [[ "$(printf '%s\n%s\n' "$MIN_RUST" "$current" | sort -V | head -n1)" != "$MIN_RUST" ]]; then
    command -v rustup >/dev/null || fail "Rust $current is older than $MIN_RUST; please upgrade it."
    info "Rust $current is older than $MIN_RUST, updating the stable toolchain..."
    rustup update stable
  fi
else
  info "Rust not found, installing via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
  info "Installed $(rustc --version)"
fi

# --- Bun --------------------------------------------------------------------
export BUN_INSTALL="${BUN_INSTALL:-$HOME/.bun}"
export PATH="$BUN_INSTALL/bin:$PATH"

if command -v bun >/dev/null; then
  info "Bun found: $(bun --version)"
else
  info "Bun not found, installing..."
  curl -fsSL https://bun.sh/install | bash
  info "Installed Bun $(bun --version)"
fi

# --- Build ------------------------------------------------------------------
BUNDLE_DIR="$PWD/src-tauri/target/release/bundle"

# Eject disk images left mounted by an earlier build (or an opened installer
# named "veilapp"); a mounted image makes the DMG step fail with
# "hdiutil: couldn't unmount ... Resource busy".
eject_stale_dmgs() {
  local dev vol
  for dev in $(hdiutil info | awk -v root="$PWD/src-tauri/target/" '
    /^image-path/ { sub(/^image-path[ \t]*:[ \t]*/, ""); hit = (index($0, root) == 1); next }
    hit && /^\/dev\/disk[0-9]+/ { d = $1; sub(/s[0-9]+$/, "", d); print d; hit = 0 }'); do
    hdiutil detach "$dev" -force -quiet || true
  done
  for vol in /Volumes/veilapp*; do
    [[ -d "$vol" ]] && { hdiutil detach "$vol" -force -quiet || true; }
  done
  rm -f "$BUNDLE_DIR"/macos/rw.*.dmg
}

info "Installing frontend dependencies..."
bun install

eject_stale_dmgs
info "Building Veil App (bun run tauri build)..."
if ! bun run tauri build; then
  # Tauri's bundled create-dmg script only retries `hdiutil detach` on exit
  # code 16, but a transient "Resource busy" (Finder/Spotlight touching the
  # fresh volume) exits with 1, so the DMG step fails intermittently. The app
  # itself is already built at this point; retry just the DMG step.
  [[ -d "$BUNDLE_DIR/macos/veilapp.app" ]] || fail "Build failed."
  for attempt in 1 2 3; do
    info "DMG bundling failed, retrying ($attempt/3)..."
    eject_stale_dmgs
    sleep 2
    bun run tauri build --bundles dmg && break
    (( attempt == 3 )) && fail "DMG bundling failed after 3 retries."
  done
fi
eject_stale_dmgs

info "Done. Build artifacts:"
ls -d src-tauri/target/release/bundle/macos/*.app src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null || true
