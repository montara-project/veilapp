---
title: Keep the Tauri plugin crates on the same minor version as their npm packages
author: Masb0ymas
date: 2026-11-23
branch: main
cost: { minutes: 8, tokens: 59875 }
id: l-680a9516
scope: repo
status: active
src: agent
touches:
  - src-tauri/Cargo.lock
---

## Decided

When `pnpm tauri build` stops with "Found version mismatched Tauri packages", bump the Rust crate in the lockfile to match npm. Do not pin npm back down.

## Watch out

- npm caret ranges pick up new minor releases of `@tauri-apps/plugin-*`, but Cargo.lock stays on the old crate version. The Tauri CLI then refuses to build until both sides share the same major/minor.
- A local build also fails at the very end, after the .app and .dmg are made, unless the updater signing key is set. `createUpdaterArtifacts` is on, so the `.app.tar.gz` needs a signature. This is an environment issue, not broken code.

In the code:

- Fix for the mismatch: `cd src-tauri && cargo update -p tauri-plugin-updater` (2.13.1 -> 2.14.1, matching npm 2.14.0)
- Local signed build: `TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/veilapp.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=... pnpm tauri build`
