# Screen-first player validation

[Issue #85](https://github.com/takahirox/msx-rust/issues/85) reorganizes the
browser player around the 4:3 display and puts cartridge, disk and cassette
in their own slots. Settings and Help open as dismissible panels while the
current emulation session continues.

## Browser evidence

Playwright captured these images from the release Wasm build in Chromium.
Viewport dimensions are in CSS pixels. The normal narrow screenshots include
the page below the fold so the stacked media area is visible. The desktop
player and cartridge picker use Retro CRT. The layout and fullscreen captures
use Clean, and separate WebGL2 tests exercise the CRT shader and its fallback.

| Viewport | Player | Native fullscreen |
| --- | --- | --- |
| Desktop 1280×800 | [Desktop](player-evidence/desktop.png) | [Desktop fullscreen](player-evidence/fullscreen-desktop.png) |
| Tablet 768×1024 | [Tablet](player-evidence/tablet.png) | [Tablet fullscreen](player-evidence/fullscreen-tablet.png) |
| Phone portrait 375×812 | [Phone portrait](player-evidence/phone-portrait.png) | [Portrait fullscreen](player-evidence/fullscreen-phone-portrait.png) |
| Phone landscape 812×375 | [Phone landscape](player-evidence/phone-landscape.png) | [Landscape fullscreen](player-evidence/fullscreen-phone-landscape.png) |

[Historical cartridge picker at the Pages prefix](https://github.com/takahirox/msx-rust/blob/ca1e8d0b60d142fcbfae8c07014454aee3d18ca0/docs/player-evidence/cartridge-picker.png)
· [Fullscreen fallback in landscape](player-evidence/fullscreen-fallback.png)

The focused interaction tests use the release build at `/` and `/msx-rust/`.
They cover keyboard opening, focus restoration, canceled and failed imports,
bundled and local cartridge boot, return to BASIC, volume in the browser output
path, viewport-fitted scale, separate disk mount and boot, read-only and dirty
disk safeguards, and cassette rate and transport. Existing browser tests
continue to verify raw pixels, interlace, joystick and keyboard input,
firmware and media state, and save isolation and recovery.

## Local checks

The checks ran on arm64 macOS 27.2.1 with Rust 1.99.0, Node.js 26.12.1 and
Playwright 1.64.1. The Wasm build used the pinned Rust linker through the
repository's macOS library-path workaround. Browser checks used Chromium
viewports and captured Web Audio output. Physical-device testing and the
optional user-supplied BIOS test were outside this run.
[macOS linker setup](deployment.md#pre-merge-validation)

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 333 passed |
| `./scripts/build.sh` | Passed with the documented macOS linker setup |
| `node scripts/check-dist.mjs` | Passed |
| `npm test` | 230 passed in 15.3 minutes |
| `git diff --check` | Passed |

## Issue #85 post-merge checklist

The original checklist below describes the player at Issue #85. For the
current local-media player, follow [Issue #87 deployment verification](deployment.md#required-post-merge-verification-pending).

After a successful GitHub Pages publication, record the deployed commit,
Actions run, browser and viewport sizes. Open the published
[MSX player](https://takahirox.github.io/msx-rust/) at representative wide and
narrow sizes. Verify the layout, MSX2 Weave BASIC startup and requested Retro
CRT default or reported Clean fallback. Open and dismiss Insert cartridge,
launch a bundled demo, and verify Settings, Help and Use Weave BASIC.
Record observed results and screenshots where available.
