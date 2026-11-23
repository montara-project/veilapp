---
title: "On macOS 27 Spotlight can't stay visible while icons hide, and dev builds lose their own icon"
author: Masb0ymas
date: 2026-11-23
branch: main
cost: { minutes: 4, tokens: 55353 }
id: l-cb9fb067
scope: repo
status: active
src: agent
touches:
  - src-tauri/src/menubar.rs
---

## Decided

- Control Center extras that the system item ids don't cover (Focus, Now Playing...) stay only through the `com.apple.controlcenter` bundle. Veil App lists that bundle when one of those extras sits right of the wall. The check uses preferred positions from defaults, so it needs no Accessibility.

## Tried and ruled out

- Keeping Spotlight: no Spotlight.app process runs on 27. It still hid with `com.apple.Spotlight` in the bundle list and with system item ids 0..4096.
- Keeping the `tauri dev` icon: the raw binary has no LaunchServices bundle id (`bundleID=[ NULL ]`), and the list matches by bundle id. A bundled app (`/Applications/veilapp.app`, `com.montara.veilapp`) did stay visible.

## Watch out

- Debug builds are ad-hoc signed and change cdhash on each build, so Accessibility trust drops. The AX-based "right of wall" list is then empty, and third-party icons right of the wall hide.

In the code: `src-tauri/src/menubar.rs` (`has_item_right_of`, `native::kept_bundle_ids`, `COVERED_EXTRAS`).
