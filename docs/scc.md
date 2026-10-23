# Classic Konami SCC audio

The Konami SCC mapper implements the K051649 MegaROM sound chip on MSX1 and
MSX2. Konami Standard is a separate mapper with no SCC. Five voices have separate
periods, counters, wave positions, linear volumes and enable bits. Four signed
32-byte waveforms are stored; voices 4 and 5 share the fourth waveform.
SCC+/SCC-I (K052539), SCC cartridge SRAM and unrelated FM expansions are unsupported.
Separate [ASCII SRAM mappers](cartridge-sram.md) provide battery-backed cartridge saves.
Importing a correctly mapped cartridge does not guarantee commercial-game compatibility.

## Memory and slot decoding

Write a value with low six bits 3Fh to 9000h–97FFh through cartridge slot 1
to enable the 9800h–9FFFh overlay. Other bank values restore ordinary ROM.
Only accesses through the selected cartridge slot with the overlay enabled reach
SCC registers. Hidden writes cannot change waves, periods, volumes or enable bits.
Visibility does not control audio: an installed, programmed SCC keeps playing and
advancing its phase across overlay and slot changes. The overlay is not
present in the Konami SCC ROM mirror at 1800h. Existing bank masks, windows,
reset banks and ROM mirrors are preserved.

| Address in the first 256-byte block | Behavior |
| --- | --- |
| 9800h–981Fh | Voice 1 signed waveform, read/write |
| 9820h–983Fh | Voice 2 signed waveform, read/write |
| 9840h–985Fh | Voice 3 signed waveform, read/write |
| 9860h–987Fh | Voices 4/5 shared signed waveform, read/write |
| 9880h–9889h | Five little-endian periods; only 12 bits retained; write-only |
| 988Ah–988Eh | Five volumes; low four bits retained; write-only |
| 988Fh | Voice enable bits 0–4; write-only |
| 9890h–989Fh | Mirrors of 9880h–988Fh |
| 98A0h–98DFh | Reserved; writes ignored, reads FFh (no SCC+ fifth waveform) |
| 98E0h–98FFh | One deformation/test latch; writes alias; reads FFh and latch FFh |

The low eight address bits decode the whole block; it repeats at 9900h through
9F00h. Write-only registers read open bus (FFh), never invented register values.
Reading the test latch therefore has a side effect, including waveform write
protection, until software writes a new test value.

## Timing, test modes and mixing

Each voice advances one wave position every `period + 1` emulated CPU T-states.
A single-cycle 32-sample waveform therefore repeats at
`3,579,545 / (32 * (period + 1))` Hz. Division ratios 0–8 stop the oscillator
and produce zero output. Writing either period byte reloads that voice's counter;
the wave position is retained unless deformation bit 5 requests a restart.
Disabling a voice mutes its output while its oscillator keeps advancing.

Deformation bits are modeled as follows:

| Bit | Behavior |
| --- | --- |
| 0 | Use the period's high four bits as the divider |
| 1 | Use its low eight bits; takes priority if both bits 0/1 are set |
| 2–4 | No modeled effect; undocumented |
| 5 | A period write also resets that voice's wave position to zero |
| 6 | Rotate waveform read addresses; shared wave 4/5 uses voice 5's phase; all wave RAM is write-protected |
| 7 | Rotate shared wave 4/5 reads using voice 4's phase; protect that waveform from writes |

Rotation moves the data originally at offset 0 toward offset 1, at the wave's
oscillator cadence. With both rotation bits set, only the first three waves
rotate, and all four remain write-protected. With bit 6 alone, shared-wave read
rotation uses voice 5's position; with bit 7 alone, it uses voice 4's position.
Electrical interactions between the two voices accessing
the shared RAM, especially combined rotation modes, are approximated by the
independent digital oscillators; bus-dependent noise is not synthesized.

The centered DAC sum uses `floor(signed_sample * volume / 16)` for each enabled
voice, including negative rounding. A fixed gain of `0.3 / 600` bounds SCC to
±0.3. The unchanged PSG path contributes at most ±0.6; their sum has headroom
within ±0.9. The existing PSG 44.1 kHz sample clock samples both chips at the
same emulated instant, preserving buffer lengths, frame debt and the Wasm ABI.
No frame-rounded pitch accumulator, audio library or browser API is in the core.

Both chips advance at instruction T-state granularity. This is deterministic
digital synthesis, not chip-perfect analog reproduction: sub-instruction bus
phases, DAC resistor tolerances, analog filtering/distortion, shared-RAM access
glitches and anti-alias filtering are not modeled. Very high pitches can alias.

Issue #39 corrects Issue #23's hidden-window silence requirement. SCC generation
and mixing depend on the installed mapper, independently of CPU register access.
Disabling the overlay or selecting another slot leaves both output and phase
uninterrupted. Explicit channel-enable and volume writes still control silence.
Reset, Eject, replacement, model or firmware changes clear waves, counters,
controls and queued PCM. Validation/allocation failures commit no machine changes,
preserving SCC and PSG state. Browser audio activation, scheduling, mute/resume,
pause and focus handling use the existing audio transport.

## Independent sources and provenance

The SCC implementation and test data were written independently from these
hardware descriptions, without copying emulator implementations or ROM bytes:

- Hiroyuki's [SCC laboratory hardware investigation](https://d4.princess.ne.jp/msx/scc/scc2.html): classic/SCC+ distinctions, measured period limits, test modes and DAC rounding.
- [SCC Sound Chip](https://ngs.no.coocan.jp/doc/wiki.cgi?page=SCC+Sound+Chip): memory mirrors and write-only register behavior; its preliminary test-mode observations are refined by the hardware investigation above.
- [MSX Wiki mapper documentation](https://direct.msx.org/wiki/ROM_mappers) and [K051649 overview](https://www.msx.org/wiki/Konami_051649): mapper activation and shared waveforms.
- [Test-register read discussion](https://www.msx.org/ja/node/63446): FFh read-latch side effect. This discussion also quotes emulator comments; no implementation code was used.

The visibility correction is inferred from pinned openMSX
[mapper decoding](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/memory/RomKonamiSCC.cc)
and [SCC synthesis](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/sound/SCC.cc):
register visibility changes independently of sound generation. The
[libmsx SCC interface and tone example](https://mori0091.github.io/libmsx/group__SCC__DEVICE.html)
likewise distinguish register exposure from channel controls and restore the old
slot before hiding the programmed chip. No upstream implementation code or example
bytes were copied. No new physical measurement or commercial-game failure was
observed; the Japanese investigation was text-corrupted during Issue #39 planning,
and the NGS reference returned 403. Delayed DAC updates, SCC-I and bus-phase
scheduling remain separate work. Optional pinned-openMSX PCM comparison has not
been performed.

The original MIT-licensed generator in
[`scc_fixture.rs`](../crates/msx-core/src/firmware/scc_fixture.rs) builds
`scc-test-cartridge.rom`. The 64 KiB MegaROM executes Z80 memory/I/O instructions,
provides enough evidence for existing Auto inference, and programs four distinct
square/saw/triangle/pulse waves with five different periods and volumes. It ships
through the existing asset generator and static distribution inventory, with
provenance in `firmware/ROM-LICENSE.txt`. It contains no commercial melodies,
recorded samples, proprietary firmware or third-party game bytes.

Open it with Konami SCC or Auto on either model. Enable audio and focus the display:
Space plays voice 1 (about 440.397 Hz); Up plays all five SCC voices; Left plays
PSG (about 588.741 Hz); Right hides the register overlay; Down maps BIOS into
page 2. Combine Right/Down to hide both; programmed sound continues. Combine Left
with Space/Up for mixed audio. Release Space/Up to explicitly silence SCC, even
while Right/Down remain held; release Left to silence PSG. The poll loop briefly
restores register access before updating channel controls, then hides the requested
access paths and attempts destructive waveform, frequency, volume and enable
writes, which must be ignored. Code and stack stay in pages 1/3 while real Z80
memory and slot-port writes change page 2. Reset restarts the fixture, and Eject
returns to BASIC.

## Automated verification

`scc` unit tests check the full register map, mirrors, write masks/open bus,
signed DAC levels, periods, independent voices, shared waves, chunk-invariant
timing, test modes and write protection. `tests/scc.rs` additionally checks slot
and mapper decoding, exact additive mixing and sample cadence, successful/failed
imports, reset/ejection/replacement, firmware/model changes and actual Z80 fixture
boot on both machines with explicit and Auto mapping. Its expected pitch and
samples are calculated from the clock, periods and synthetic wave values. On both
models, every PCM sample across overlay, slot and combined transitions/restoration
is checked against an independent clock/DAC oracle and an equal-elapsed-cycle
visible control, with one/five voices and hidden register-write isolation. Intervals
cross sample/frame boundaries; irregular tick partitions have an additional PCM
oracle. Simultaneous PSG mixing is checked while visibility changes.

`tests/scc.spec.js` imports that same generated ROM through the real browser file
input on MSX1/MSX2 with explicit/Auto selection, at root and Pages subpaths. It
observes completed native Web Audio buffers from compiled Wasm, checking finite
bounded samples, independently calculated DAC levels and pitch, five voices,
PSG alone/together, overlay/slot/combined hiding and restoration, enable-mask silence,
failed imports, mute/resume, pause, reset and Eject. Existing PSG, cartridge, C-BIOS,
BASIC and gallery suites remain part of `npm test`. Deterministic capture copies every Wasm
PCM frame across visibility transitions, checking signed DAC levels and phase
against the independently calculated `32 * 254` cycle period. Scheduled Web Audio
checks use fresh completed buffers after queued audio drains; phase comparisons
join only consecutive emulated frames, so scheduler gaps cannot invent edges.
Physical listening is optional. Issue #39 requires no post-merge verification.

Issue #23's published-site check remains **pending after merge**: verify the
successful main Pages deployment, then use this fixture on the published site,
record nonzero SCC/PSG and silence on mute/ejection with deployment URL/run.
Local tests do not establish that deployment succeeded.

## Local verification record (2026-10-21)

The implementation was checked with the pinned Rust 1.99.0 toolchain:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed, 94 tests |
| `./scripts/build.sh` | Passed with the local linker workaround below |
| `node scripts/check-dist.mjs` | Passed, including C-BIOS/demo hashes and notices |
| `npm ci` | Passed |
| `npx playwright test tests/scc.spec.js` | Passed, 8 SCC cases |
| `npm test` | Passed, all 58 browser tests (including all 8 SCC cases), 7.0 minutes |
| `git diff --check` and local documentation links | Passed |

On this macOS host, the pinned toolchain's `rust-lld` initially failed to locate
its installed `libLLVM.dylib`. An ignored `target/local-linker/rust-lld` shell
wrapper set `DYLD_LIBRARY_PATH` to that toolchain's `lib` directory and executed
its original linker. The build used
`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER="$PWD/target/local-linker/rust-lld"`
with the pinned toolchain's `bin` first in `PATH`. No toolchain files, repository
build settings, dependency versions or deployment configuration were changed.

## Issue #39 verification (2026-10-24)

The new `hidden_registers_preserve_pcm_pitch_and_phase_against_visible_control`
regression was run before changing production code, which matched evaluation
baseline `ae711ff47e0ebd0aeb2afdf93326d7e0a71eee95`. It failed at the first
hidden sample: actual `[0.0]`, visible control `[-0.056500003]`. The same regression
passes after removing visibility from SCC mixing.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed after simplifying a test expression flagged for precedence |
| `cargo test --workspace --locked` | Passed, 149 tests |
| `./scripts/build.sh` | Passed with the documented local toolchain/linker setup |
| `node scripts/check-dist.mjs` | Passed |
| `npm ci` / `npx playwright install chromium` | Passed |
| `npx playwright test tests/scc.spec.js` | Passed, 8 cases, 3.4 minutes |
| `npm test` | Passed, all 88 browser tests, 9.1 minutes |
| `git diff --check` and local Markdown links | Passed |

The initial build used a system Rust ahead of rustup and could not find Wasm std;
putting the pinned 1.99.0 toolchain first in `PATH` resolved that mismatch. Its
linker then hit the existing macOS `libLLVM.dylib` lookup problem. The ignored
`target/local-linker/rust-lld` wrapper described above resolved it. Toolchain,
dependency and production build configuration were unchanged.

Local review followed `docs/review-guidelines.md`, checking the complete diff
against Issue #39: register decoding, bank masks/mirrors and ROM visibility are
unchanged; the private mixer no longer accepts visibility; the public Wasm ABI is
unchanged. Native/browser evidence covers hidden output and phase, write isolation,
explicit silence, five/shared-wave voices, PSG mixing and existing lifecycle and
failed-import behavior. No blocking findings remain in the local review. PR review,
publication, physical measurement, commercial-game validation and optional pinned
openMSX comparison were not performed. Issue #39 requires no post-merge work.
