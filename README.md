# Veil App

A macOS menu bar app that keeps your menu bar clean and tidy — a lightweight alternative to [ClearBarMac](https://clearbarmac.app/) and [Hidden Bar](https://github.com/dwarvesf/hidden).

Veil App lives in the menu bar. Click its icon to open a small translucent panel that lists every app that has an icon in your menu bar — with its icon, name, and live memory usage — and lets you **hide the menu bar icons you don't want to see** with one click.

## Features

- **Menu bar popover** — a tray icon opens a compact panel drawn with the native macOS popover material (Liquid Glass on macOS 26, `NSVisualEffectView` on older systems), adapting to light/dark appearance automatically; it hides itself when it loses focus (or press `Esc`).
- **Running apps at a glance** — every running third-party app with its icon, name, and live memory usage, refreshed every 5 seconds. Apps using ≥ 1 GB get a red badge. No special permissions needed.
- **Hide menu bar icons** — one button hides every icon except Veil App, Wi-Fi, Battery, Spotlight, Control Center and the Clock; click again to bring everything back.
- **App management** — click a card to activate an app, hover and click ✕ (or right-click) to quit it, hold `⌥` to force-quit.
- **Sort by Name or Memory.**
- **Universal builds** — CI publishes a single binary that runs natively on Intel and Apple Silicon.

## How "Hide icons" works

_Hide icons_ uses the public-API **wall** trick (the same one Hidden Bar and Ice use). Veil App owns a blank status item, the _wall_, placed directly left of its own icon. Expanding the wall pushes every icon to its left off the visible bar; collapsing it brings them back. Nothing needs the Accessibility permission, and the wall disappears with the app, so icons always come back when Veil App quits.

On launch, Veil App reads where macOS keeps Wi-Fi, Battery and Spotlight (their `NSStatusItem Preferred Position` defaults). It then seeds its own positions so the wall and its icon sit just left of whichever of those is leftmost: `… [wall][Veil App] Wi-Fi · Battery · Spotlight · Control Center · Clock`. While hidden, only **Veil App, Wi-Fi, Battery, Spotlight, Control Center and the Clock** stay visible. Items turned off in System Settings are skipped.

A third-party icon that sits _between_ those system items also stays visible, because a single wall can only hide what is to its left. Cmd-drag such an icon to the left of the wall to hide it as well. The seeded positions are only rewritten if they are missing, the wall ends up right of the icon, or a kept item ends up left of the wall, so your own Cmd-drag arrangement survives relaunches.

> Changing the alpha of other apps' status item windows (`CGSSetWindowAlpha`) does not work: the window server silently ignores it for windows owned by another process.

> **macOS 26 note:** Tahoe hosts all status items under the Control Center process, so individual menu bar icons can no longer be attributed to apps through the window list. Veil App therefore lists _Accessory-policy_ apps (background/menu bar apps) as its best public-API approximation.

## Tech stack

| Layer      | Choice                                                    |
| ---------- | --------------------------------------------------------- |
| Framework  | [Tauri 2](https://v2.tauri.app) (Rust + WebView)          |
| Frontend   | TypeScript + Vite (vanilla, no framework)                 |
| Tooling    | [Bun](https://bun.sh)                                     |
| macOS APIs | `objc2` / `objc2-app-kit` (`NSStatusItem`, `NSWorkspace`) |
| Memory     | `libproc` (`proc_pid_rusage` → phys footprint)            |

## Getting started

**Prerequisites:** macOS with Xcode Command Line Tools, [Rust](https://rustup.rs), and [Bun](https://bun.sh).

```bash
bun install
bun run tauri dev      # run the app with hot reload
bun run tauri build    # produce a local .app and .dmg
```

> The app has no Dock icon and no main window — look for its icon in the menu bar.

## Releasing

A GitHub Actions workflow (`.github/workflows/release.yml`) builds a **universal macOS binary** and publishes a GitHub Release whenever a `v*` tag is pushed:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

Signing is optional: add the `APPLE_CERTIFICATE`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, and `APPLE_TEAM_ID` repository secrets to ship signed, notarized builds. Without them the build is unsigned and users will see a Gatekeeper warning on first launch (right-click → Open, or `xattr -cr Veil\ App.app`).

## Project structure

```
src/                     # Panel UI (TypeScript + CSS)
src/main.ts              # Grid rendering, sorting, IPC calls, mock data for browser preview
src-tauri/
  src/lib.rs             # App entry: tray icon, panel window, IPC commands
  src/menubar.rs         # Hide/show via the wall status item + seeded item positions
  src/apps.rs            # Running third-party apps, memory, icons, quit/activate
.github/workflows/release.yml  # Tag-push release pipeline (universal macOS)
```

### IPC commands

| Command                 | Purpose                                           |
| ----------------------- | ------------------------------------------------- |
| `list_apps`             | Menu bar apps (Accessory policy) + memory + icons |
| `quit_app(pid, force)`  | Terminate / force-terminate by PID                |
| `activate_app(pid)`     | Bring an app to the foreground                    |
| `toggle_menu_bar_icons` | Expand/collapse the wall                          |
| `hide_panel`            | Hide the popover (bound to `Esc`)                 |

## Roadmap ideas

- Per-app RAM sparklines sampled from a Rust background thread
- Persist "hidden" state across launches
- Custom tray icon
