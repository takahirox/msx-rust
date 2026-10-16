# Issue #7 UI validation

Source: [Redesign the browser emulator UI](https://github.com/takahirox/msx-rust/issues/7).
Validated locally on 2026-10-17 in the implementation worktree. This record does
not establish that a merge, GitHub Actions run, or production deployment occurred.

The application now centers the native Canvas display beneath a compact toolbar.
Pause / Resume, Reset, audio, Open ROM / Eject, and Fullscreen are grouped together.
Keyboard focus has a visible outline and cue, and Tab leaves the Canvas. Starters
resume BASIC and restore focus; long help, BIOS/legal details, and the transcript
use native expandable sections. Touch users are explicitly told that typing needs
a physical keyboard. ROM files still stay in memory without uploads or storage.
Rust, firmware, dependencies, and deployment configuration were not changed.

## Automated results

Environment: macOS arm64, rustup Rust 1.99.0, Node.js 24.12.0, Python 3.13.13,
Playwright 1.64.1 and Chromium 156.0.8078.4. The pinned rustup toolchain was placed
first in PATH, as described in README, to avoid the system Homebrew compiler.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed: 26 integration tests (6 BASIC, 7 CPU, 13 hardware) |
| `./scripts/build.sh` | Initial default-linker attempt failed to locate `libLLVM.dylib`; passed with the existing documented macOS linker wrapper, then passed again on final sources |
| `node scripts/check-dist.mjs` | Passed: unchanged eight-file inventory, relative links, ROM sizes/headers and import-free Wasm |
| `npm ci`; `npx playwright install chromium` | Passed |
| `npm test` | Passed: all 20 Chromium tests |
| `node --check web/app.js`; `node --check tests/browser.spec.js` | Passed |
| `git diff --check` | Passed |

The build used `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER` with the ignored
`target/pages-linker.sh` wrapper from the existing
[deployment instructions](deployment.md#pre-merge-validation). No toolchain or
system installation was modified. The GitHub Pages workflow remains unchanged.

The browser suite preserves the original functional checks and covers:

- Canvas BASIC output, editing, BREAK, multiline paste, Pause / Resume and Reset.
- BASIC example and graphics starters, plus a real AudioContext sound starter,
  fresh-buffer PSG silence after Stop tone, mute/re-enable, suspension/resume and
  a recoverable audio initialization failure. No physical-speaker claim is made.
- Open ROM's actual file chooser, original cartridge boot, invalid/oversized ROM
  rejection, ejection, authored 32 KiB system-ROM boot, and return to Built-in BASIC.
  File loading still makes no upload or third-party requests.
- Focus restoration and visible focus cues, Tab navigation, no typing captured
  outside the display, and allowing the browser's Ctrl+V paste shortcut.
- Native fullscreen, unavailable/rejected API fallbacks, visible exit controls,
  Escape exiting the fallback, and 4:3 scaling at desktop, mobile portrait and
  landscape sizes. Fallback layout is also checked while resizing the active view.
- Closed-by-default help, BIOS/legal and transcript disclosures, then opening
  them and reaching the actual controls and transcript.
- Desktop 1280×800, tablet 768×1024, mobile 375×812 and narrow 320×640 layouts:
  screen inside viewport, 4:3 ratio, no horizontal overflow, nonoverlapping toolbar
  buttons with at least 44px height, and a compact desktop/tablet toolbar.
- Real static serving at `/msx-rust/`, with all asset requests under the prefix,
  no failed requests/page errors, BASIC interaction and Canvas output.
- Delayed Wasm loading and both HTTP and invalid-Wasm initialization failures,
  with visible status and appropriately disabled commands.

## Screenshot evidence

These are automated captures from the final successful test run. Copies are
committed here so they survive the implementation node's checkpoint. The complete
set is regenerated in ignored `test-results/` by `npm test`; the existing workflow
uploads it as `browser-evidence`. The screenshots were inspected locally for
readability and layout, without requiring subjective owner approval.

- [Desktop, 1280px](ui-evidence/desktop.png)
- [Mobile, 375px](ui-evidence/mobile.png)
- [Expanded mobile help, BIOS options and transcript](ui-evidence/mobile-details.png)
- [Focused fallback in landscape, 812×375](ui-evidence/focus-landscape.png)

## Optional checks not performed

Firefox/Safari, physical devices, physical-speaker listening and the optional
external C-BIOS integration were not run. The existing core tests and authored
browser system-ROM fixture passed; no proprietary ROM was obtained or bundled.

## Required post-merge verification — pending

After merge, confirm the existing Pages workflow publishes the merged commit at
<https://takahirox.github.io/msx-rust/>. Record the workflow/deployment URL and
commit, then check that the actual static site and relative JS/CSS/Wasm assets
load successfully and the redesigned UI boots. Local build and project-prefix
checks do not prove publication. This is a post-merge requirement, not an
outstanding pre-merge acceptance gate. No push, PR, comment, merge or deployment
was performed by this implementation node.
