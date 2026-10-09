# Veil App

<p align="center">
  <a href="https://github.com/montara-project/veilapp/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/montara-project/veilapp?display_name=tag&sort=semver"></a>
  <a href="https://github.com/montara-project/veilapp/actions/workflows/release.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/montara-project/veilapp/release.yml?label=release"></a>
  <a href="https://github.com/montara-project/veilapp/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/montara-project/veilapp/total"></a>
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-green.svg"></a>
  <img alt="Platforms" src="https://img.shields.io/badge/platform-macOS-lightgrey">
</p>

A macOS menu bar app that keeps your menu bar clean and tidy — a lightweight alternative to [ClearBarMac](https://clearbarmac.app/) and [Hidden Bar](https://github.com/dwarvesf/hidden).

Veil App lives in the menu bar. Click its icon to open a small translucent panel that lists every app that has an icon in your menu bar — with its icon, name, and live memory usage — and lets you **hide the menu bar icons you don't want to see** with one click.

## Features

- **Menu bar popover** — a tray icon opens a compact panel drawn with the native macOS popover material (Liquid Glass on macOS 26, `NSVisualEffectView` on older systems), adapting to light/dark appearance automatically; it hides itself when it loses focus (or press `Esc`).
- **Tray menu** — right-click the tray icon for a native menu: **Settings…** opens the settings window, **Quit Veil App** exits.
- **Settings** — _General_: launch Veil App at login, show/hide the memory readout in the panel. _About_: what the app is, its version and identifier.
- **Running apps at a glance** — every running third-party app with its icon, name, and live memory usage, refreshed every 5 seconds. Apps using ≥ 1 GB get a red badge. No special permissions needed.
- **Hide menu bar icons** — one button hides every icon except Veil App, Wi-Fi, Battery, Spotlight, Control Center and the Clock; click again to bring everything back.
- **App management** — click a card to open an app, hover and click ✕ (or right-click) to quit it, hold `⌥` to force-quit. Apps that only open from their own menu bar icon get that icon clicked for you (needs the Accessibility permission, see [Opening apps from the panel](#opening-apps-from-the-panel)).
- **Sort by Name or Memory.**
- **Universal builds** — CI publishes a single binary that runs natively on Intel and Apple Silicon.

## How "Hide icons" works

_Hide icons_ uses the public-API **wall** trick (the same one Hidden Bar and Ice use). Veil App owns a blank status item, the _wall_, placed directly left of its own icon. Expanding the wall pushes every icon to its left off the visible bar; collapsing it brings them back. Hiding needs no permission, and the wall disappears with the app, so icons always come back when Veil App quits.

On launch, Veil App reads where macOS keeps Wi-Fi, Battery and Spotlight (their `NSStatusItem Preferred Position` defaults). It then seeds its own positions so the wall and its icon sit just left of whichever of those is leftmost: `… [wall][Veil App] Wi-Fi · Battery · Spotlight · Control Center · Clock`. While hidden, only **Veil App, Wi-Fi, Battery, Spotlight, Control Center and the Clock** stay visible. Items turned off in System Settings are skipped.

A third-party icon that sits _between_ those system items also stays visible, because a single wall can only hide what is to its left. Cmd-drag such an icon to the left of the wall to hide it as well. The seeded positions are only rewritten if they are missing, the wall ends up right of the icon, or a kept item ends up left of the wall, so your own Cmd-drag arrangement survives relaunches.

> Changing the alpha of other apps' status item windows (`CGSSetWindowAlpha`) does not work: the window server silently ignores it for windows owned by another process.

> **macOS 26 note:** Tahoe hosts all status items under the Control Center process, so individual menu bar icons can no longer be attributed to apps through the window list. Veil App therefore lists _Accessory-policy_ apps (background/menu bar apps) as its best public-API approximation.

## Opening apps from the panel

Clicking a card reopens the app, the way clicking it in the Dock does. That is enough for apps that answer with a window (Stats and Wallper open their main or settings window).

Some apps ignore that request and only open from their own menu bar icon: tray apps that hide their window on close (WaDesk), and apps whose whole UI is a popover on the icon (Alfred, FineTune). For those, Veil App waits 0.8 s for a window, and if none shows up it clicks the app's menu bar icon for you:

1. If icons are hidden, the wall collapses first. An icon has to be on the bar to be clicked, so the icons appear for a moment.
2. Veil App finds the app's icon through the Accessibility API (`AXExtrasMenuBar`) and posts a real left click on it, then puts the cursor back where it was. A real click is used because many tray implementations react to mouse events, not to the accessibility "press" action.
3. If the icons were hidden and the app now shows a window, the wall expands again, so the icons end up hidden as before. An app that opens a popover or menu on its icon instead keeps the icons shown, because the popover anchors to the icon; press **Hide icons** again when you are done.

This is the only feature that needs the **Accessibility permission**. macOS asks for it the first time a card needs the icon click; enable Veil App under _System Settings → Privacy & Security → Accessibility_ and click the card again. Without the permission everything else keeps working, and such cards simply do nothing.

Limits:

- An app with several menu bar icons gets its first one clicked.
- Apple's own menu bar apps (Weather, input menu) are not listed in the panel, so they cannot be opened from it.
- On macOS 26 every status item is hosted by Control Center, so an app's icon may not be found and the click is skipped.
- Unsigned builds get a new code identity on every build, so macOS may ask for the permission again after an update.

## Tech stack

| Layer      | Choice                                                                               |
| ---------- | ------------------------------------------------------------------------------------ |
| Framework  | [Tauri 2](https://v2.tauri.app) (Rust + WebView)                                     |
| Frontend   | TypeScript + Vite (vanilla, no framework)                                            |
| Tooling    | [Bun](https://bun.sh)                                                                |
| macOS APIs | `objc2` / `objc2-app-kit` (`NSStatusItem`, `NSWorkspace`), Accessibility + `CGEvent` |
| Memory     | `libproc` (`proc_pid_rusage` → phys footprint)                                       |
| Login item | [tauri-plugin-autostart](https://v2.tauri.app/plugin/autostart/) (`SMAppService`)    |

## Getting started

### Install from source (one command)

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/montara-project/veilapp/main/scripts/install.sh)"
```

The [install script](scripts/install.sh) installs [Rust](https://rustup.rs) (via rustup) and [Bun](https://bun.sh) if they're missing, clones the repo into `~/veilapp` (set `VEILAPP_DIR` to change it; an existing checkout is updated instead), and runs `bun run tauri build`. When it finishes, open `src-tauri/target/release/bundle/dmg/veilapp_<version>_<arch>.dmg` and drag Veil App into Applications.

Already have a checkout? Run `./scripts/install.sh` from it.

**Requires** macOS with the Xcode Command Line Tools (`xcode-select --install`).

### Development

```bash
bun install
bun run tauri dev      # run the app with hot reload
# produce a local .app and .dmg; updater artifacts are skipped because the
# signing private key only exists in CI
bun run tauri build -c '{"bundle":{"createUpdaterArtifacts":false}}'
bun run lint           # oxlint + eslint (import sorting via eslint-plugin-perfectionist)
bun run format         # oxfmt, formats in place
bun run release        # release-it: bump version, tag, push (triggers the release build)
```

Commits must follow [Conventional Commits](https://www.conventionalcommits.org) (enforced by commitlint via a husky `commit-msg` hook), and `lint-staged` runs oxlint/eslint/oxfmt on staged files on every commit. `bun run release` bumps the version in `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and `Cargo.lock` in one go, commits as `chore: release vX.Y.Z`, tags `vX.Y.Z`, and pushes.

> The app has no Dock icon and no main window — look for its icon in the menu bar.

## Releasing

A GitHub Actions workflow (`.github/workflows/release.yml`) builds a **universal macOS binary** and publishes a GitHub Release whenever a `v*` tag is pushed:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

### Auto-update

Installed apps check `https://github.com/montara-project/veilapp/releases/latest/download/latest.json` at startup and offer an in-app update (Settings → About). Bump the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and `package.json`, then push a new tag — the running app downloads, verifies, and installs the update on its own, no rebuild or reinstall needed.

This requires two repository secrets, already in place:

- `TAURI_SIGNING_PRIVATE_KEY` — the minisign private key (`~/.tauri/veilapp.key`, generated with `bunx tauri signer generate -w ~/.tauri/veilapp.key`). It signs the `.app.tar.gz` updater artifacts and never leaves GitHub Secrets. **Losing it means existing installs can never receive a verified update again**, so keep a backup of the file outside the machine.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the password chosen when the key was generated.

### Optional Apple code signing

Add the `APPLE_CERTIFICATE`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, and `APPLE_TEAM_ID` repository secrets to ship signed, notarized builds. Without them the build is unsigned and users will see a Gatekeeper warning on first launch (right-click → Open, or `xattr -cr Veil\ App.app`). In-app updates are unaffected: the updater writes the bundle to disk directly, so no quarantine attribute is applied.

## Project structure

```
src/                     # Panel UI (TypeScript + CSS)
src/main.ts              # Grid rendering, sorting, IPC calls
src/settings.ts          # Settings window: General + About tabs
settings.html            # Settings window markup
src-tauri/
  src/lib.rs             # App entry: tray icon + menu, panel & settings windows, IPC commands
  src/menubar.rs         # Hide/show via the wall status item + seeded item positions
  src/apps.rs            # Running third-party apps, memory, icons, quit/activate
  src/ax.rs              # Accessibility: find and click another app's menu bar icon
  src/settings.rs        # Persisted preferences (settings.json in the app config dir)
scripts/install.sh       # One-command build: installs Rust/Bun if missing, clones, builds
.github/workflows/release.yml  # Tag-push release pipeline (universal macOS)
```

### IPC commands

| Command                       | Purpose                                                   |
| ----------------------------- | --------------------------------------------------------- |
| `list_apps`                   | Menu bar apps (Accessory policy) + memory + icons         |
| `quit_app(pid, force)`        | Terminate / force-terminate by PID                        |
| `activate_app(pid)`           | Reopen an app; click its menu bar icon if no window shows |
| `toggle_menu_bar_icons`       | Expand/collapse the wall                                  |
| `hide_window(label)`          | Hide the panel or settings window (bound to `Esc`)        |
| `get_settings`                | Read persisted preferences                                |
| `set_show_memory_usage(show)` | Persist the panel memory-usage preference                 |

## Roadmap ideas

- Per-app RAM sparklines sampled from a Rust background thread
- Persist "hidden" state across launches
- Custom tray icon
