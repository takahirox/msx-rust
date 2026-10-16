# Issue #8 MegaROM validation record

## Completed post-merge Pages verification

Verified on 2026-10-17 (JST) after PR #10 merged with merge commit
`6606118c81a4606946e029ce732eb680064cd7f1`. The source Issue #8 was freshly
confirmed open immediately before the merge, which required the reviewed PR
head `1661af108e2eab4b8553aab85ecf351a891092ff` and used a merge commit.

[Pages workflow run 38936910588](https://github.com/takahirox/msx-rust/actions/runs/38936910588)
completed successfully for that merge commit, including its browser checks and
deployment. Playwright with Chromium 156.0.8078.4 then tested the actual deployed
site at <https://takahirox.github.io/msx-rust/>:

- HTML, CSS, JavaScript and Wasm returned HTTP 200; built-in BASIC reached READY.
- Original MIT-licensed fixtures from `tests/browser.spec.js` loaded through the
  real cartridge file input: 64 KiB ASCII8, 128 KiB ASCII16, 256 KiB Konami,
  512 KiB Konami SCC and 1024 KiB ASCII8. Each executed code in its final bank,
  filled the Canvas with the expected backdrop, and changed color while Space
  was held before restoring the original color on release.
- Auto requested explicit mapper selection for the 64 KiB image without the
  obsolete "too large" rejection; manual selection loaded it successfully.
- Reset restored execution. A malformed 64 KiB image produced a nonfatal import
  error while the active cartridge continued responding to Space.
- Ejection restored BASIC, which executed `PRINT 42` and returned to READY.
- No page errors or failing HTTP responses occurred. Observed requests were
  exclusively GET requests within the deployed site; no ROM upload occurred.

Required post-merge verification for Issue #8 is complete; no defects were found
by these checks. Historical pending statements below describe pre-merge evidence.
External copyrighted ROMs, SCC audio and other browsers were not tested.

## Integration with the merged Issue #7 UI

Before PR publication on 2026-10-17, integrated `main` at
`9e79661a899823317a63c6a4faa94e148ebff6d3` (PR #9). Preserved the redesigned
toolbar, disclosures, audio controls, fullscreen and focus behavior. Mapper
selection lives under **ROMs & advanced system options**; **Open ROM** still
opens the cartridge picker. Replaced the obsolete size-rejection assertion and
retained both the mapper and UI browser coverage. Eject can cancel a pending
file read without allowing its eventual completion to reload the cartridge.

Re-ran `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo test --workspace --locked`: all passed, including all 34 Rust integration
tests. `./scripts/build.sh` passed with the documented macOS linker wrapper;
`node scripts/check-dist.mjs`, `npm ci`, `npx playwright install chromium`, and
`npm test` passed. The combined browser suite passed all **27 Chromium tests**,
including real banked file imports, allocation-failure recovery, project-prefix
hosting, mobile layouts and fullscreen. JavaScript syntax and whitespace checks
also passed. No workflow changes were needed. External BIOS/game ROMs and other
browsers were not tested. Required deployed Pages verification remains pending
until after merge; local results do not establish deployment success.

## Original implementation validation

Source: [Remove the arbitrary cartridge ROM size limit and support bank-switched MegaROMs](https://github.com/takahirox/msx-rust/issues/8).
Local validation date: 2026-10-17. Pre-merge worktree validation only; no push, PR,
comment, merge or deployment was performed.

Environment: macOS arm64, pinned rustup Rust 1.99.0 (`b940084d7`), Node.js 24.12.0,
Python static servers, Playwright 1.64.1 / Chromium 156.0.8078.4.

| Command / check | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | All 34 integration tests passed: 8 mapper, 13 hardware/machine, 7 CPU, 6 BASIC |
| `./scripts/build.sh` | Release Wasm and site passed with the existing macOS linker wrapper documented in [deployment.md](deployment.md#pre-merge-validation) |
| `node scripts/check-dist.mjs` | Passed: unchanged eight-file asset inventory, ROM headers/sizes, relative paths, import-free Wasm and required transfer exports |
| `npm ci` | Passed; locked development dependencies, no vulnerabilities reported |
| `npx playwright install chromium` | Passed; browser available |
| `npm test` | All 14 Chromium tests passed, including real MegaROM file imports and `/msx-rust/` hosting |
| `node --check web/app.js`; `node --check tests/browser.spec.js` | Passed |
| `git diff --check` | Passed |

The host initially selected Homebrew Rust 1.94.0, which lacked the Wasm target.
Selecting the pinned rustup compiler in PATH resolved that mismatch. Its macOS
linker could not locate `libLLVM.dylib`; an outer DYLD_LIBRARY_PATH did not help.
The repository's already documented ignored `target/pages-linker.sh` wrapper fixed
that environment problem without source, compiler or CI configuration changes.

Eight new Rust tests exercise exact ASCII8/ASCII16/Konami/Konami SCC register
ranges and ignored writes, initial banks, bank wrapping, distinct Konami mirrors,
SCC enable/ROM-overlay decoding, slot visibility, reset, replacement/ejection,
system-ROM separation and transactional validation failures. Synthetic images
from 64 KiB to 4 MiB verify last-bank reads without truncation. Z80 fixtures on
all four mappers switch a bank and execute code from it. Existing plain cartridge,
header/entry-point, keyboard, video, PSG and BASIC regressions also pass.

Five browser cases select original generated 64/128/256/512/1024 KiB MegaROM files
through the real file input and compiled Wasm path. Auto requests explicit mapper
selection. Code in the final switched bank controls Canvas backdrop pixels and
reacts to Space. This remains responsive after malformed-header, partial-image,
wrong-extension and invalid-system-ROM imports. Each case resets, replaces the
mapper, loads a plain cartridge, ejects and returns to built-in BASIC. File imports
cause no upload or third-party requests. The actual `/msx-rust/` server test also
imports and runs a 128 KiB ASCII16 fixture under the project prefix.

A raw Wasm test reserves beyond initial linear memory, creates a fresh writable
view after growth, writes its last byte, and checks invalid transfer lengths,
mapper IDs, zero lengths and an unrepresentable u32 reservation without a trap.
A browser fault-injection test bounds only its test Wasm instance to 4 MiB, then
requests an 8 MiB reservation to exercise a genuine try_reserve_exact failure
without exhausting host memory. It separately simulates File.arrayBuffer allocation
failure; the existing banked cartridge still responds to keyboard input. A deferred file read completed after
Eject cannot reload its cartridge. Actual host-wide memory exhaustion was not
attempted. These fixtures contain original MIT-licensed bytes only, with no external
ROM downloads or additional deployable ROM assets.

SCC sound/register hardware, SRAM mapper variants and universal game compatibility
are not claimed. Auto deliberately recognizes validated plain layouts only; known
MegaROMs require manual selection. See the [mapper table and compatibility limits](../README.md#rom-loading-and-legal-guidance).
Optional external C-BIOS, third-party games, other browsers and physical audio
listening were not tested for this change. No workflow changes were needed.

## Issue #8 required post-merge verification — pending

After merge and the Pages deployment, verify the actual deployed UI and Wasm at
<https://takahirox.github.io/msx-rust/>. Select a permitted MegaROM with its documented
mapper, observe execution and confirm the obsolete 48 KiB rejection is absent.
Record the deployment run, commit and actual result; file follow-up Issues for
defects. This implementation node does not deploy or perform post-merge checks.
Local build/subpath results do not establish deployed results, and this pending
check must not block pre-merge review or merge.

---

# Issue #3 validation record

Source: [Implement the complete initial browser-playable MSX1 emulator](https://github.com/takahirox/msx-rust/issues/3).
Local validation date: 2026-10-17. This is a pre-merge worktree record, not a claim
that GitHub Actions ran or a public site was published.
For the current UI controls and Issue #7 results, see [UI validation](ui-validation.md).
The original application results below are retained for provenance. For the Pages
scaffold's integration results, see [deployment validation](deployment.md#pre-merge-validation).

## Review fixes and revalidation

On 2026-10-17, integrated `main` at `618911c8eeabd4ab0f653a35f0ec43d19da0637a`
and resolved all seven conflicting files. Retained the pinned Rust compiler,
clean `dist/` build, actual project-prefix static server, deployment guide and
main-only Pages deployment. The combined workflow also retains Rust formatting,
Clippy and tests, plus the `browser-evidence` upload.

Added a deterministic PSG muting regression for each of the three channels:
first require tone samples above 0.05, then write zero amplitude, allow 250 ms
of emulated filter decay, and check every fresh sample over 100 ms is finite
and below 0.0001 absolute amplitude. Earlier samples are discarded.

The browser test activates Web Audio and verifies `BEEP` produces a peak above
0.05 and more than ten positive zero crossings. After `SOUND 8,0`, it allows
500 ms for queued playback (at most approximately 150 ms), filter decay and frame
margin. It then requires at least twelve newly scheduled buffers (about 200 ms)
to finish playback in a running AudioContext, with every sample finite and below
0.0001 absolute amplitude. Per-buffer peaks and completion events prevent stale
tone counters or a stopped scheduler from being mistaken for successful muting.

Revalidated on the macOS/Rust/Node/Chromium environment listed below:

| Command / check | Actual result after conflict resolution |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | All 26 integration tests passed: 7 CPU, 13 hardware/machine, 6 BASIC |
| `./scripts/build.sh` | Initial macOS linker load failed; passed using the documented local wrapper in [deployment.md](deployment.md#pre-merge-validation) with `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER` |
| `node scripts/check-dist.mjs` | Passed; eight-file deployment inventory and Wasm/ROM checks |
| `npm ci`; `npx playwright install chromium`; `npm test` | All seven Chromium tests passed, including tone/muting and actual `/msx-rust/` hosting |
| `go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 .github/workflows/pages.yml` | Passed |
| Build fingerprints | All three hashes match the original application build below |

The optional external C-BIOS tests and human listening were not repeated. No
push, GitHub merge or deployment was performed. Production verification remains
pending; the automated audio checks establish signal generation and muting,
without claiming a physical-speaker listening result.

## Original application environment and completed checks

macOS arm64; rustup Rust 1.99.0 (`b940084d7`), wasm32-unknown-unknown;
Node 24.12.0; Python 3.13.13; Playwright 1.64.1; headless Chromium 156.0.8078.4.
The test suite starts an isolated local HTTP server and refuses to reuse a service
already listening on its port.

| Command / check | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 25 integration tests passed: 7 CPU, 12 hardware/machine, 6 BASIC; no failures or ignored tests |
| `./scripts/build.sh` | Release Wasm and static site built; original boot/test ROMs generated from source |
| `node scripts/check-dist.mjs` | Exact eight-file deployment inventory, ROM sizes/header, relative links and import-free Wasm verified |
| `npm ci` | Locked development dependencies installed; no runtime JS dependencies bundled |
| `npx playwright install chromium` | Browser available |
| `MSX_TEST_BIOS=target/validation/cbios_main_msx1.rom npm test` | Eight browser tests passed, including optional C-BIOS integration |
| `cargo run --release -p msx-core --example check_bios -- target/validation/cbios_main_msx1.rom` | Unmodified C-BIOS booted the authored cartridge; 600 frames, VDP text, PPI Space input and PSG output passed |
| Workflow YAML | Parsed locally; verify/deploy jobs, conditional main-branch deployment, artifact dependency and Pages/OIDC permissions reviewed against GitHub's documented workflow |
| `git diff --check` | Passed after staging final source/documentation |
| Browser screenshots | BASIC prompt/output, graphics curve, built-in cartridge boot and C-BIOS cartridge boot inspected visually |

The default `npm test` suite has seven tests and needs no external ROM. Setting
`MSX_TEST_BIOS` adds the eighth test; it is not fetched by the tests or used by CI.
Screenshot evidence is regenerated in `test-results/basic.png`, `graphics.png`,
`cartridge.png`, and (when opted in) `cbios-cartridge.png`. CI uploads that directory.

Browser checks actually performed:

- Booted the compiled Wasm to an interactive BASIC screen, typed/stored/listed/ran
  `10 FOR I=1 TO 5 : PRINT I : NEXT I`, and recognized 1–5 directly from Canvas pixels.
- Replaced/deleted numbered lines and edited a command with Home/End/arrows/Backspace.
- Paused/resumed/reset the machine and interrupted an infinite BASIC program with Escape.
- Rendered the graphics sine curve through Graphics II VRAM and checked its colored pixels.
- Unlocked a real AudioContext via a button, issued `BEEP`, and observed nonzero,
  oscillating samples in AudioBufferSources connected to the running audio destination.
- Loaded our cartridge through the file picker, saw its text/sprite, rejected an
  invalid ROM without replacing it, and ejected back to BASIC. The Rust cartridge test
  separately verified Space-controlled PSG amplitude and nonzero samples.
- Loaded an original authored system-ROM fixture through the system picker, observed
  its VDP output, verified native BASIC service ports were disabled, and returned to BASIC.
  File loading caused no upload/non-GET or third-party network requests.
- Booted an unmodified, locally supplied C-BIOS with our cartridge in the actual browser.
- Pasted a program longer than the core input FIFO and verified its final line/output.
- Loaded the same artifact under a simulated `/msx-rust/` project prefix, with its
  contents unchanged, and typed a BASIC command successfully.

Build fingerprints from the validated local build (compiler-dependent, not release signatures):

```text
msx_wasm.wasm      3d41eb3a179434b0369614716d5446634ef3bc40647267bbb8c654f40998779e
test-cartridge.rom 118c6242a2071a4ad1f4e63a52b86bd9625420b6aa1b41b9e8690fd203f2278d
weave-boot.rom     a49cc03789a09b0a395b01756925903a1ac4f5f31b4fc1a3747f2a4c6b89d079
```

## Reproduce optional C-BIOS verification

The [C-BIOS Association](https://cbios.sourceforge.net/) explicitly identifies its
BIOS as BSD-2-Clause and states it contains no BASIC. The license notice was reviewed
before obtaining the archive. Only the ROM was executed; no C-BIOS source was used
to implement the emulator. The archive/ROM remain under ignored `target/validation/`
and are absent from committed sources and the deployable artifact.

```sh
mkdir -p target/validation
curl -fL https://downloads.sourceforge.net/project/cbios/cbios/0.29/cbios-0.29a.zip -o target/validation/cbios-0.29a.zip
shasum -a 256 target/validation/cbios-0.29a.zip
# Expected: c585a2f4a10b397e460a95043d59ac79cf4442fbb45a6e14a1e27ef9febaed73
unzip -jo target/validation/cbios-0.29a.zip 'cbios-0.29a/roms/cbios_main_msx1.rom' -d target/validation
cargo run --release -p msx-core --example check_bios -- target/validation/cbios_main_msx1.rom
MSX_TEST_BIOS=target/validation/cbios_main_msx1.rom npm test
```

## Manual browser smoke flow

1. Build/serve using the README commands. Confirm the visible blue display says
   `WEAVE BASIC 0.1` and `READY`. Click the display before typing.
2. Type `10 FOR I=1 TO 5 : PRINT I : NEXT I`, Enter, `LIST`, Enter, `RUN`, Enter.
   Confirm visible lines 1–5. Replace line 10 with `10 PRINT 42` and run again.
   Enter `10` alone and `LIST` to verify deletion. Exercise arrow/Home/End editing.
3. Click **Graphics demo** and confirm a yellow curve appears. Press Enter to return.
   Optionally run the numbered graphics example from README.
4. Click **Enable audio**; type `BEEP` and listen for a steady tone. Type `SOUND 8,0`
   and confirm silence. Audio should remain silent before user activation.
5. Expand **ROMs & advanced system options**, download `test-cartridge.rom`, then
   choose it with **Open ROM**. Confirm title and yellow sprite. Click the display and hold Space: hear the tone, then release to silence. Check that focus
   loss releases held keys. Reset should boot the cartridge again; Eject returns to BASIC.
6. Optionally select your lawfully obtained 32 KiB system ROM, then a compatible
   plain cartridge. Files must stay local. **Built-in BASIC** unloads both inputs.
7. Pause, confirm video execution stops, and Resume again. Reload to verify the
   original BASIC prompt returns without retaining user ROM data.

## Limitations and unperformed checks

The implemented built-in interpreter covers the required language constructs and
VDP/PSG demos, but is a Rust firmware service, not a Z80 Microsoft BASIC implementation
or a full MSX BIOS API replacement. Self-contained plain cartridges and C-BIOS boot
were verified. Original proprietary BIOS/BASIC and third-party games were not tested
or obtained; compatibility beyond the supplied software is not established.

CPU tests are deterministic and cover the implemented prefix families, but are not
exhaustive Z80 silicon conformance. Bus phases/M1 wait states, undocumented quirks,
analog video/audio fidelity, and raster-edge behavior remain approximate. Out-of-scope
mappers, expanded slots and peripherals are listed in README. No reference-emulator
or physical-MSX comparison, Firefox/Safari/mobile testing, or physical-speaker
listening was performed. Automated Web Audio waveform/destination verification passed;
manual listening remains unperformed, not reported as passed.

Early checks exposed a Homebrew/rustup toolchain mismatch and an unrelated server on
the original test port; toolchain selection and isolated-server configuration fixed
them. Canvas digit recognition was tightened to exclude program-listing prefixes.
Graphics review found multicolor addressing, tile-mode CLS and transparent-sprite
collision issues; fixes have regression coverage. No failing checks remain in the
reported final automated suites.

## Required post-merge verification — pending

Target: <https://takahirox.github.io/msx-rust/>. No push, PR, merge, comment or deployment
was performed by this implementation node. A read-only `GET /repos/takahirox/msx-rust/pages`
returned HTTP 404; current publication/configuration could not be established.

Ensure repository **Settings → Pages → Source** is **GitHub Actions**, and the
`github-pages` environment permits deployment from `main`. After merge and a successful
workflow, record the actual published URL and repeat the smoke flow against that URL,
including keyboard interaction, local ROM selection, and audio activation/listening.
Check that Wasm and all relative assets load without errors. Pre-merge artifact and
subpath checks do not establish that publication succeeded. Keep this pending until
performed; do not close it based on a local build alone.
