# Veil App

A macOS menu bar app that keeps your menu bar clean and tidy — a lightweight, open take on [ClearBar](https://clearbar.app) / [Hidden Bar](https://github.com/dwarvesf/hidden).

Veil App lives in the menu bar. Click its icon to open a small translucent panel that lists every app that has an icon in your menu bar — with its icon, name, and live memory usage — and lets you **hide the menu bar icons you don't want to see** with one click.

## Features

- **Menu bar popover** — a tray icon opens a compact panel drawn with the native macOS popover material (Liquid Glass on macOS 26, `NSVisualEffectView` on older systems), adapting to light/dark appearance automatically; it hides itself when it loses focus (or press `Esc`).
- **Menu bar apps at a glance** — the actual menu bar items, read through the Accessibility API (macOS 26 hosts all status items under Control Center), mapped to their running apps with icon, name, and live memory usage, refreshed every 5 seconds. Apps using ≥ 1 GB get a red badge. Needs the Accessibility permission — without it the panel falls back to listing background (Accessory-policy) apps and shows an *Enable…* banner.
- **Hide menu bar icons** — one button collapses your icon clutter behind an invisible spacer; click again to bring everything back.
- **App management** — click a card to activate an app, hover and click ✕ (or right-click) to quit it, hold `⌥` to force-quit.
- **Sort by Name or Memory.**
- **Universal builds** — CI publishes a single binary that runs natively on Intel and Apple Silicon.

## How "Hide icons" works

With the Accessibility permission granted, *Hide icons* targets each hosted menu bar window individually through the private SkyLight (CGS) framework — the same class of API menu bar managers like Bartender and Ice rely on. The keep-list is explicit: **Veil App itself, Battery, Spotlight and Wi-Fi stay visible** (Spotlight identified as the anonymous window between Battery and Wi-Fi); every other icon — third-party apps, Clock, Control Center — is hidden. Press *Show icons* to restore, and icons are always restored when the app quits.

Without Accessibility access, it falls back to the public-API **wall** trick: Veil App owns an invisible blank status item (the *wall*); expanding it pushes every icon to its left out of the visible bar, collapsing brings them back.

> Because hiding uses a private framework, this app is not suitable for the Mac App Store. It has no effect on other apps' processes — only the visibility of their hosted menu bar windows.

> **macOS 26 note:** Tahoe hosts all status items under the Control Center process, so individual menu bar icons can no longer be attributed to apps through the window list. Veil App therefore lists *Accessory-policy* apps (background/menu bar apps) as its best public-API approximation.

## Tech stack

| Layer      | Choice                                              |
| ---------- | --------------------------------------------------- |
| Framework  | [Tauri 2](https://v2.tauri.app) (Rust + WebView)    |
| Frontend   | TypeScript + Vite (vanilla, no framework)           |
| Tooling    | [Bun](https://bun.sh)                               |
| macOS APIs | `objc2` / `objc2-app-kit` (`NSStatusItem`, `NSWorkspace`), Accessibility API |
| Memory     | `libproc` (`proc_pid_rusage` → phys footprint)      |

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
  src/menubar.rs         # Per-icon hide/show via SkyLight (CGS) + wall spacer fallback
  src/apps.rs            # Menu bar app list (AX items → running apps), memory, icons, quit/activate
  src/ax_menubar.rs      # Accessibility API: reads status items via the Control Center process
.github/workflows/release.yml  # Tag-push release pipeline (universal macOS)
```

### IPC commands

| Command                 | Purpose                                        |
| ----------------------- | ---------------------------------------------- |
| `list_apps`             | Menu bar apps (Accessory policy) + memory + icons |
| `quit_app(pid, force)`  | Terminate / force-terminate by PID             |
| `activate_app(pid)`     | Bring an app to the foreground                 |
| `toggle_menu_bar_icons` | Expand/collapse the wall                       |
| `hide_panel`            | Hide the popover (bound to `Esc`)              |
| `accessibility_granted` | Whether the Accessibility permission is on     |
| `request_accessibility` | Show the system Accessibility prompt           |

## Roadmap ideas

- Per-app RAM sparklines sampled from a Rust background thread
- Persist "hidden" state across launches
- Custom tray icon
