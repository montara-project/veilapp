---
title: 'On macOS 27 hiding icons uses the system allow-list, not the wall'
author: Masb0ymas
date: 2026-11-23
branch: main
cost: { minutes: 8, tokens: 101171 }
id: l-f0cdda1a
scope: repo
status: active
src: agent
touches:
  - src-tauri/src/apps.rs
  - src-tauri/src/menubar.rs
  - README.md
---

## Decided

- macOS 27 (Golden Gate) draws the whole menu bar in one window (`MenuBarAgent`) and ejects a status item expanded to 10,000pt instead of letting it push icons off. Expanding the wall hides nothing, and Show has nothing to undo.
- On macOS 27+ hiding uses the allow-list behind assessment (exam) mode: `MBAssessmentModeConfiguration initWithAllowedSystemItems:allowedBundleIdentifiers:` and `MBAssessmentModeAssertion activateWithConfiguration:completionHandler:` / `invalidate`, from `/System/Library/PrivateFrameworks/MenuBarClientCore.framework`. This is the route Hidden Bar 1.13 takes. It was verified on 27.2.1: unlisted icons hide, `invalidate` brings them back, and process exit restores the bar.
- The collapsed 8pt wall stays as the boundary. Apps whose icon sits right of it, plus Veil App itself, go on the list. System items 0..64 are always passed.
- The framework is resolved at runtime (dlopen + class/selector checks, gated on OS major >= 27). If it is missing, the code falls back to the wall.

## Tried and ruled out

- Hiding per-icon windows: per-app status item windows (layer 25) no longer exist on 27. CGWindowList shows only `MenuBarAgent` / Window Server menubar windows.
- Keeping the wall with a smaller expanded length: the item is ejected rather than clipped, so no length makes it push icons off.

## Watch out

- macOS still hides some of its own items (Spotlight, BentoBox) while the list is active, even with every system item allowed. There is no API to keep them.
- Without Accessibility, positions can't be read, so only Veil App stays visible.
- A dev run (raw binary, no bundle id) hides its own tray icon too. Use the panel button to show again.
- If macOS refuses the list, the code fails open: HIDDEN goes back to false, but the panel label is not re-emitted.

In the code: `src-tauri/src/menubar.rs` (`mod native`, `set_hidden`), `src-tauri/src/apps.rs` (`bundle_ids_with_icon_from`), test `native_hiding_resolves_on_macos_27`.
