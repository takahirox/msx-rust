# AY-3-8910 envelope timing

Issue [#37](https://github.com/takahirox/msx-rust/issues/37) corrects a sixteenfold
slowdown in the shared native/Wasm PSG. The existing logarithmic DAC, mixer,
44,100 Hz sample accumulator, DC blocker and SCC mixing remain in place. Machine
still owns the chips and frame samples; the browser copies those samples into its
existing Web Audio buffers. No audio ABI or lifecycle changes are required.

## Clock contract and evidence

The [MSX2 Technical Handbook, chapter 5](https://raw.githubusercontent.com/Konamiman/MSX2-Technical-Handbook/master/md/Chapter5a.md)
gives 256 × EP PSG clocks per envelope ramp. The PSG input is CPU/2. Sixteen
visible levels therefore require **32 × EP CPU cycles per level** and **512 × EP
CPU cycles per ramp** for positive periods. The transcription contains numerical
typos, so its printed frequency examples are not the timing oracle.

[openMSX's AY implementation](https://raw.githubusercontent.com/openMSX/openMSX/master/src/sound/AY8910.cc)
corroborates the rate and describes a counter shared with its YM model, with two
internal steps per visible AY level. It documents period clamping and R13 phase
retention partly from YM2149 measurements, assumed to apply to AY. These boundary
rules are provisional here. [MAME's commentary](https://raw.githubusercontent.com/mamedev/mame/master/src/devices/sound/ay8910.cpp)
instead describes zero envelope period as twice the speed of one. Zero-as-one is
our chosen compatibility policy, not a settled physical AY fact. References were
inspected on 2026-10-23; no upstream implementation code was copied.

Tone toggles still occur every 16 × tone-period CPU cycles; noise shifts every
32 × noise-period cycles. Neither rate changes with EP. Previously an envelope
level consumed 32 × EP of the already divided CPU/16 clocks, or 512 × EP CPU
cycles. Correcting that extra division leaves the other clocks intact.

## Deterministic writes

The implementation's internal envelope index runs from 31 down to zero; adjacent
indices map to the same visible level. Every CPU/16 clock adds two counter units;
the internal-step threshold is `2 * max(EP, 1)`. A crossing subtracts the threshold
and advances one internal index. This represents phase at half-tick resolution,
including odd counter values introduced by period shortening. Machine continues to
apply port writes during instruction execution before advancing chips by that
instruction's cycles; subinstruction bus phases are not modeled.

- R11 and R12 immediately update their respective bytes. Each write keeps the
  shape index and clamps the counter to at most the new threshold minus one.
  Increasing a period keeps the existing counter; decreasing it can schedule an
  internal step at the next divider edge. Separate byte writes are not atomic.
- Every R13 write, even the same value, resets the internal index, direction and
  hold state. It preserves both the period counter and CPU/16 divider phase.
- The counter keeps running while a shape holds. Retriggering a held shape uses
  that phase, just as retriggering a running shape does.
- EP=0 uses EP=1 timing. Reset alone restores index, counter, divider, sampling
  phase and DC filter to their initial state.

A shape write need not change the visible level at the next internal step: that
first step is hidden by the duplicated AY levels. Repeated R13 writes every 16
CPU cycles with EP=1 keep a descending shape visibly at 15. Writes every 24 cycles
see levels 15, 14, 15, 14 immediately before writes at 24, 48, 72, 96. A reported
YM dip does not imply an AY dip on every internal step.

The table specifies absolute CPU timestamps from an aligned reset. Shape 8 means
descending repeat, shape 9 descending hold, and shape 12 ascending repeat.

| Setup and write | Expected visible output |
| --- | --- |
| EP=1, shape 8 at cycle 0 | 15 through 31; 14 at 32; 0 at 480 through 511; 15 at 512 |
| EP=4, shape 8; R11=1 at 48 | 15 at 63, 64, 65 and 79; 14 at 80; 13 at 112 |
| EP=4, shape 8; R11=1 at 304 | 13 before/after the write and at 320; 12 at 336 |
| EP=2, shape 8; R11=4 at 48 | 15 through 95; 14 at 96; 13 at 224 |
| EP=1, shape 8; R12=1 at 96 | 12 through 8319, including hidden step at 4208; 11 at 8320 |
| EP=0101h, shape 8; R11=2 then R12=0 at 48 | Internal step at 64; 15 through 95; 14 at 96 |
| Same setup, R12=0 then R11=2 at 48 | Internal step at 80; 15 through 111; 14 at 112 |
| EP=2, shape 8; R13=8 at 64+p, p=0..15 | Immediately 15; hidden step at 96; first visible fall at 128 |
| EP=2, shape 8; R13=12 at 47 | Immediately 0; hidden step at 64; first visible rise at 96 |
| EP=1, held shape 9; R13=9 at 512+p, p=0..15 | Immediately 15; first visible fall at 544 |
| EP=2, held shape 9; R13=9 at 1119 | Immediately 15; first visible fall at 1152 |

## Independent coverage

`crates/msx-core/tests/envelope.rs` replaces the old slow envelope expectation in
`hardware.rs`. Its visible-shape oracle uses explicit ramp sequences, independently
of the production shape logic. All sixteen shapes run at EP=1, 2, 0101h and FFFFh
through three ramps, checking one cycle before, at and after each boundary,
including repeat, alternate and hold. Write vectors cover the table above, period
shortening/byte order at all sixteen divider phases, and zero versus one. Existing
tone/noise tests remain separate.

The sampled-envelope oracle calculates sample timestamps as
`ceil(sample_number * 3579545 / 44100)` and visible levels from `32 * EP`, then
applies the specified DAC and DC filter. It checks each channel, sample count,
finite bounded output and ignores fixed-volume low bits when envelope selection
is set. Separate checks isolate fixed-volume channels from envelope writes.
Private-state tests compare every PSG field and identical PCM under large versus
irregularly partitioned ticks with the same write schedule, both with and without
SCC, and reproduce fresh state/PCM after reset.

The shared original MIT fixture `tests/fixtures/envelope-probe.hex` contains only
independently authored Z80 instructions. Generated system ROMs enter it at 0100h;
it writes PSG ports A0h/A1h, selects EP=0101h and shape 8, then HALTs with interrupts
disabled. A minimal generated CD-header sub-ROM satisfies the MSX2 paired-ROM
contract. No commercial BIOS, music, game bytes or upstream fixture is used.

The native Machine test measures at least four repeat edges and identical reset
PCM on each model. `tests/envelope.spec.js` loads the same bytes through the normal
local firmware picker on MSX1 and MSX2 at `/` and `/msx-rust/`. It copies twenty
frames from the real compiled Wasm audio ABI, checking cadence, bounds and reset
reproducibility, then measures contiguous actual scheduled Web Audio buffers,
respecting the existing latency cap that may skip frames during catch-up. Repeat edges
must be **1621 or 1622 samples apart**:
`512 * 257 * 44100 / 3579545 = 1621.115...`. The old sixteenfold slowdown needs
about 25,938 samples per repeat and cannot produce one repeat in the twenty-frame
window (about 14,719 samples).

## Validation record (2026-10-23–24)

Before changing `psg.rs`, the focused test
`cargo test --locked -p msx-core --test envelope aligned_envelope_uses_32_cpu_cycles_per_level`
failed on the baseline implementation: cycle 32 returned level 15 instead of 14.
It passes after correction. This was an actual baseline run, independent of the
historical PR #36 results.

Full-check results are recorded in [the validation log](validation.md#issue-37-ay-envelope).
No pinned openMSX trace comparison, physical AY measurement, commercial-title
listening test or speaker test was performed. The digital tests establish the
stated clock/write contract; they do not settle disputed chip edge cases or prove
analog fidelity or general software compatibility. No post-merge verification is
required by this Issue.
