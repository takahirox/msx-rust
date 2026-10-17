# Issue #13 post-merge verification

Verified on 2026-10-17 at 10:25 UTC after merging
[PR #14](https://github.com/takahirox/msx-rust/pull/14) with merge commit
`266385f7f8fcd3417b0f110e25d72ad84af97ab6`.

Issue #13 was read immediately before the merge and confirmed open. The merge
used GitHub's merge-commit API with the reviewed head SHA
`08f093c997cb6997f40021241270ebe7f467f074`; GitHub accepted the merge without
bypassing checks or reviews.

## Publication

[Workflow run 39024724707](https://github.com/takahirox/msx-rust/actions/runs/39024724707)
for the merge commit completed successfully, including the build, automated
validation and GitHub Pages deployment. The published site is
<https://takahirox.github.io/msx-rust/>.

The served assets matched the downloaded `github-pages` artifact from this run
byte for byte:

| Asset | SHA-256 |
| --- | --- |
| `app.js` | `941d5e4d2ad1e650382d74743fa2e59ceb929e2d531a50652e87a4db89b3e52f` |
| `msx_wasm.wasm` | `21a5a72011e50d7bcc2bb8cef0b6a0ec486a1594d14b53ba10e748dfc6aa1e5c` |

## Deployed browser verification

Playwright 1.64.1 / Chromium 156.0.8078.4 loaded the published HTTPS site and
confirmed the built-in BASIC booted. The mapper remained at its default Auto
selection throughout the checks. The original MIT-licensed `megaRom` and
`ambiguousRom` builders from `tests/browser.spec.js` generated local fixture
bytes; no commercial ROM assets were used.

| Original 128 KiB fixture | Displayed inference | Execution result |
| --- | --- | --- |
| `authored-128-2.rom` | `Auto: ASCII8, estimate` | Passed |
| `authored-128-3.rom` | `Auto: ASCII16, estimate` | Passed |
| `authored-128-4.rom` | `Auto: Konami, estimate` | Passed |
| `authored-128-5.rom` | `Auto: Konami SCC, estimate` | Passed |

For each fixture, code in the final bank filled all 256 × 192 pixels with
RGB `(212, 193, 84)`. Holding Space changed all pixels to `(212, 82, 77)`;
releasing Space restored the initial color. This checks observable banked
execution and keyboard response after import through the deployed file picker.

Importing the ambiguous fixture then displayed `Ambiguous mapper evidence`
and preserved the running cartridge's full-screen backdrop. Auto remained
selected. No browser page errors occurred.

The required post-merge publication and deployed Auto checks for Issue #13
passed. These synthetic fixtures do not establish universal cartridge accuracy
or commercial-game hardware compatibility.
