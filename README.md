# Veil App

A macOS menu bar app that keeps your menu bar clean and tidy — a lightweight, open take on [ClearBar](https://clearbar.app) / [Hidden Bar](https://github.com/dwarvesf/hidden).

Veil App lives in the menu bar. Click its icon to open a small translucent panel that lists every running app with its icon and live memory usage, and lets you **hide the menu bar icons you don't want to see** with one click.

## Features

- **Menu bar popover** — a tray icon opens a compact, translucent panel; it hides itself when it loses focus (or press `Esc`).
- **Running apps at a glance** — icon, name, and real memory usage for every app, refreshed every 5 seconds. Apps using ≥ 1 GB get a red badge.
- **Hide menu bar icons** — one button collapses your icon clutter behind an invisible spacer; click again to bring everything back.
- **App management** — click a card to activate an app, hover and click ✕ (or right-click) to quit it, hold `⌥` to force-quit.
- **Sort by Name or Memory.**
- **Universal builds** — CI publishes a single binary that runs natively on Intel and Apple Silicon.

## How "Hide icons" works

macOS offers no public API to hide other apps' status items, so Veil App uses the same public-API trick as Hidden Bar: it owns an invisible blank status item (the **wall**). When you press *Hide icons*, the wall expands to ~800 px and macOS pushes every icon to its left out of the visible bar. Pressing *Show icons* collapses the wall and everything returns.

Both the tray icon and the wall are normal `NSStatusItem`s created through `objc2-app-kit` — no private APIs, no hacks. You can still Cmd-drag either item to arrange the bar the way you like; macOS remembers the order.

## Tech stack

| Layer      | Choice                                              |
| ---------- | --------------------------------------------------- |
| Framework  | [Tauri 2](https://v2.tauri.app) (Rust + WebView)    |
| Frontend   | TypeScript + Vite (vanilla, no framework)           |
| Tooling    | [Bun](https://bun.sh)                               |
| macOS APIs | `objc2` / `objc2-app-kit` (`NSStatusItem`, `NSWorkspace`) |
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
  src/menubar.rs         # The "wall" status item that hides/shows menu bar icons
  src/apps.rs            # Running-app enumeration, memory, icons, quit/activate
.github/workflows/release.yml  # Tag-push release pipeline (universal macOS)
```

### IPC commands

| Command                 | Purpose                                        |
| ----------------------- | ---------------------------------------------- |
| `list_apps`             | Running regular apps + memory + icons (base64) |
| `quit_app(pid, force)`  | Terminate / force-terminate by PID             |
| `activate_app(pid)`     | Bring an app to the foreground                 |
| `toggle_menu_bar_icons` | Expand/collapse the wall                       |
| `hide_panel`            | Hide the popover (bound to `Esc`)              |

## Roadmap ideas

- Include menu-bar-only apps (`Accessory` policy) with a filter toggle
- Per-app RAM sparklines sampled from a Rust background thread
- Persist "hidden" state across launches
- Custom tray icon
