# Issue #3 validation record

Source: [Implement the complete initial browser-playable MSX1 emulator](https://github.com/takahirox/msx-rust/issues/3).
Local validation date: 2026-10-17. This is a pre-merge worktree record, not a claim
that GitHub Actions ran or a public site was published.

## Environment and completed checks

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
3. Click **Try graphics** and confirm a yellow curve appears. Press Enter to return.
   Optionally run the numbered graphics example from README.
4. Click **Enable audio**; type `BEEP` and listen for a steady tone. Type `SOUND 8,0`
   and confirm silence. Audio should remain silent before user activation.
5. Download/select `test-cartridge.rom`. Confirm title and yellow sprite. Click the
   display and hold Space: hear the tone, then release to silence. Check that focus
   loss releases held keys. Reset should boot the cartridge again; Eject returns to BASIC.
6. Optionally select your lawfully obtained 32 KiB system ROM, then a compatible
   plain cartridge. Files must stay local. **Built-in BASIC** unloads both inputs.
7. Pause, confirm video execution stops, and Start again. Reload to verify the
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
