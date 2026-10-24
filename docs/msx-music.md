# Optional MSX2 internal MSX-MUSIC

Issue [#65](https://github.com/takahirox/msx-rust/issues/65) adds one explicitly
optional internal-style profile. It does not change machine generation or claim
a full historical MSX2 machine. A fresh machine has no FM hardware installed.

## Installation and slots

Supply a separate **16 KiB internal-profile firmware** through the browser's
MSX-MUSIC controls or `Machine::install_msx_music(Vec<u8>)`. The image is mapped
byte-for-byte, read-only, at 4000h–7FFFh in expanded slot **3-1**. It is not a
game cartridge, system BIOS or disk ROM. Slot 1 remains the cartridge/SCC-I,
slot 2 the disk interface, 3-0 the sub-ROM and 3-3 the RAM mapper. FFFFh's
inverted secondary-slot readback and write latch retain precedence on all
expanded slot-3 selections. No FM-PAC memory registers, enable latch, banking or
SRAM are provided. MSX-AUDIO and MSX1 FM expansions are outside this profile.

The explicit selection establishes the hardware profile. Only exact image length
is validated: a universal header/signature cannot identify all internal firmware.
No production identification string is injected, no bytes are patched, and no
FM-BIOS call is intercepted. Lawfully supplied compatible firmware/software is
needed for vendor APIs. C-BIOS/Weave BASIC do not gain FM BASIC commands.

Core and Wasm consume an owned staged image only after validation. A failed
profile, length, file read or reservation preserves execution and the installed
image. Browser imports share the existing generation token, cancellation, SRAM
save and allocation boundary. Local bytes are never uploaded or stored; the
filename is a display label only. Installation/removal restarts selected firmware,
retaining cartridge/SRAM, disk interface, disk changes, cassette/position and
system firmware selection. Reset clears registers, phases, envelopes, LFO and
noise state but retains installation. Existing firmware/cartridge changes retain
installation. MSX1 retains the image inactive, with unconnected FM ports and no
FM output. Returning to MSX2 resets/reactivates it. Other media retain their
existing model-transition rules. Explicit removal disables FM and releases its ROM.

Wasm adds `msx_install_music_rom(length, profile)` (profile **1** only),
`msx_music_profile()` (0/1 installation, even when inactive on MSX1), and
`msx_remove_music_rom()`. It reuses `msx_upload`/`msx_cancel_upload`, error strings
and the unchanged `msx_audio`/`msx_audio_len` 44.1 kHz mono f32 ABI.

## Ports and timing

On installed MSX2, OUT **7Ch** selects an address and OUT **7Dh** writes data.
Only the low byte of the CPU port decodes. Both ports read FFh. Firmware slot
visibility never gates I/O or sound. Repeated data writes retain the address;
repeated key-on values do not retrigger an already held key.

One chip clock equals one CPU T-state: **3,579,545 Hz**, including existing MSX
opcode fetch waits. Synthesis updates every **72 clocks**. CPU bus operations are
applied in instruction order, then devices advance by that instruction's elapsed
cycles, following the existing PSG/SCC instruction-level contract. Individual OUT
bus edges and internal time-multiplexed DAC slots are not modeled. Partial clock
and sample remainders survive instructions and frames. Pause freezes execution;
browser mute only gates Web Audio scheduling, so emulated phase continues.

The datapack prescribes at least **12 chip clocks after an address write** and
**84 after a data write**. Our diagnostics wait beyond these values. Faster writes
are deterministically accepted in CPU order immediately, with no queue, dropped
write or automatic CPU wait. This policy does **not** establish undocumented
silicon write-edge behavior. Address values >=40h are ignored on data writes;
19h–1Fh, 29h–2Fh and 39h–3Fh alias 10h–16h, 20h–26h and 30h–36h, following the
inspected emu2413 model. Reserved 08h–0Dh have no synthesis effect. At test register
0Fh, modeled experimental bits force zero envelope attenuation (bit 0), reset
LFO phases (bit 1), reset operator phase before each increment (bit 2), and speed
up LFOs (bit 3). Higher bits do nothing. This follows upstream's experimental
model; software should keep test bits zero for normal operation.

## Synthesis and adaptation audit

The dependency-free portable Rust implementation adapts the pinned MIT-licensed
[emu2413 a1d5120](https://github.com/digital-sound-antiques/emu2413/tree/a1d5120903b75f93d3eb7690e34a29c0b4add903).
The source and lookup tables retain attribution; the complete upstream LICENSE
ships with the source and distribution as `emu2413-LICENSE.txt`. Its license
permits adaptation and binary redistribution. No C library, build-time network,
unsafe code, runtime dependencies or browser APIs enter the core.

Included behavior: nine independent two-operator channels; F-number/block pitch,
volume, key and sustain controls; programmable instrument and fifteen presets;
feedback, full/half sine waves, attack/decay/sustain/release and key-on damping;
key rate/level scaling; amplitude modulation and vibrato; six melodic channels
plus bass drum, snare, tom, cymbal and hi-hat, with coupled phases and noise.
The adaptation keeps upstream's 19-bit phase, 10-bit logarithmic sine, exponential
conversion, 7-bit envelope steps, multiplier, preset, AM and vibrato tables and
18 noise shifts per internal sample. State is fixed-size and all clock arithmetic
is integer. AM wraps at its full table period to avoid a long-run overflow jump.

Changes from upstream: omit other chip types, allocation, update-request/pointer
caches, stereo/panning/masking and its sinc resampler; recompute operator parameters
from registers per internal clock. Sum only voices active in the current mode,
avoiding stale melodic/rhythm output when switching modes. Wide signed mixing
avoids intermediate i16 overflow. A held clock/72 sample is resampled at the
existing rational 44.1 kHz boundary and scaled by **1/131072** before addition;
combined output clamps to [-1,1] when FM is installed. Disabled-device PSG/SCC
PCM and sample counts remain unchanged. This gain is an emulator mix policy,
not a measured analog motherboard ratio. No analog reconstruction filter or
physical DAC distortion is claimed. High-frequency resampling can alias.

The manufacturer's derived transcription mixes documentation and author analysis;
physical-chip experiments and models still have discrepancies. This selected
model and exact C trace agreement do not establish analog fidelity or vendor
firmware/software compatibility.

## Original diagnostic fixtures

`music-diagnostic-firmware.rom` is an original MIT 16 KiB fixture with a deliberate
**WVFMTEST** marker at 4018h and an actual Z80 `LD A,65h; RET` routine at 4100h.
It is not a vendor FM BIOS. `music-test-cartridge.rom` is original MIT Z80 code,
executing from cartridge page 2. It probes all four subslots of primary slot 3,
finds the marker, checks inverted FFFFh readback and ROM protection, reads/calls
the installed firmware, checks unconnected port reads, programs a custom sine
through spaced OUTs, hides the ROM and halts with sound continuing. Exact success
RAM at C100h–C105h is **01 08 57 65 FF A5** and the backdrop is green (MSX2 RGBA
36,218,36,255). A missing fixture fails red. Both ROMs are generated from
`crates/msx-core/src/firmware/music_fixture.rs` and included in the distribution.

Browser reproduction: install the diagnostic firmware under MSX-MUSIC, select
**Original diagnostic loader**, choose **Plain** mapper, then open the test
cartridge. Enable audio. Reset repeats the test; Remove MSX-MUSIC silences it.
No proprietary ROM or game is necessary.

`tests/fixtures/music-disk.py` generates a separate original disk fixture by adding
spaced FM OUTs and PSG programming before the existing original Philips ROM's
init. Paired with the original SCC-I sector fixture, native and browser tests
exercise disk reads, accessory RAM execution, SCC-I/PSG/FM and an exact 512-byte
modified-sector export together. Run `python3 tests/fixtures/music-disk.py` to
regenerate its sparse hex; no vendor disk ROM is used.

## Validation

Native tests cover absent/present hardware, exact mapping, slots/boundaries,
FFFFh precedence, ROM protection, hidden sound, reset/removal, media/firmware
retention and inactive MSX1. Synthesis tests compare exact integer C reference
traces for every preset, release, each percussion voice, all nine key-on
transients, live instrument updates, AM/vibrato/feedback/scaling and rhythm
transitions; source, hashes,
driver, regeneration and tolerance rationale are in
[tests/reference/README.md](../tests/reference/README.md). Independently specified
custom-sine pitch uses `Fnum * 2^block * clock / (72 * 2^19)`, checking crossing
times within 1.1 internal samples (one sample plus phase-table quantization
margin), peak bounds and eight volume steps' 24 dB attenuation. Browser crossings
allow two 44.1 kHz samples for held native samples plus output-grid quantization;
raw eight-step volume peaks must be 120–135 versus 2000–2048 at full volume,
allowing logarithmic table/phase quantization around the expected ~1/16 ratio. Tests cover repeated writes, aliases/reserved/test registers,
custom updates, channel independence, rhythm switching and long partitioned
clock runs spanning multiple 16-bit envelope-counter wraps. Mixing tests require exact PCM equality across elapsed-cycle
partitions and disabled-device paths, exact rational sample counts and finite,
bounded sums. Allocation tests exercise commit/reset/removal without allocation.

Compiled-Wasm browser tests run at `/` and `/msx-rust/`: real file picker imports,
CPU diagnostic video, scheduled PCM at 44.1 kHz, mute/resume, pause/reset/removal,
malformed and failed file reads, bounded-memory real Wasm reservation failures,
canceled/stale imports, SRAM safeguards, private local data and simultaneous
Philips/SCC-I/PSG/FM execution. These observe Canvas/AudioContext, not speakers.

Additional physical-chip comparisons, genuine FM BIOS and commercial software
were not tested and are optional. No required post-merge verification is added.

### Local execution record (2026-10-24)

| Check | Observed result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 274 passed; the strengthened long-run AM/vibrato/feedback partition case also passed its subsequent focused rerun |
| `./scripts/build.sh` | Passed with pinned Rust 1.99.0 and the local linker setup below |
| `node scripts/check-dist.mjs` | Passed; 41 deployment files, unchanged C-BIOS and required notices |
| `npm test` | 182 passed in 13.9 minutes, including eight new FM checks at both site roots |
| Pinned C reference execution | 52 rows / 1664 integer samples matched exactly |
| `python3 tests/reference/missing-bus.py` | Both expected baseline-bus assertion failures observed |
| Fixture regeneration, local Markdown links and `git diff --check` | Passed |

README setup (`npm ci`, Chromium installation and the pinned-toolchain PATH) was
used. The initial Homebrew Rust build lacked the Wasm standard library. With the
pinned toolchain, this host's `rust-lld` then lacked a runtime LLVM library search
path. An ignored, local linker wrapper set `DYLD_LIBRARY_PATH` to the pinned
sysroot's `lib` immediately before invoking that same `rust-lld`; the build then
passed. No toolchain workaround or alternate compiler was added to the project.

Initial validation also exposed a test that inspected unused video-buffer
capacity, Clippy precedence/chunk lints, asynchronous import test races and a
regression harness missing bundled firmware in its temporary copy. Those were
corrected and the affected checks rerun successfully. No failing mandatory check
remains. The final long-run test keeps AM, vibrato and feedback active across
multiple LFO/envelope-counter wraps.

Source-Issue and full-diff self-review covered synthesis, independent ROM/I/O,
transactionality, media/model transitions, audio, diagnostics, reference evidence
and redistribution notices. It also corrected the deployment inventory count.
No blocking source finding remains. This implementation node creates no PR,
comments or merge; review of the actual downstream PR remains required before
merge. Physical-chip, genuine vendor FM-BIOS and commercial-software checks were
not performed. Required post-merge verification: none.

## Behavioral references

- [MSX datapack MSX-MUSIC hardware chapter](https://map.grauw.nl/resources/datapack/Vol2-3.1MSX-MUSICHardware.pdf): internal/external distinction, write ports and recovery clocks.
- [Pinned openMSX MSXMusic.cc](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/sound/MSXMusic.cc): separately mapped ROM, reset and timed I/O; no openMSX code copied.
- [YM2413 manufacturer-derived transcription](https://d4.princess.ne.jp/msx/datas/OPLL/YM2413AP.html): controls and programming, including author analysis.
- [andete's physical-chip experiments](https://www.smspower.org/Development/YM2413ReverseEngineeringNotes2018-05-13): rhythm/noise research and unresolved instrument details.
- [Pinned emu2413 C implementation](https://github.com/digital-sound-antiques/emu2413/blob/a1d5120903b75f93d3eb7690e34a29c0b4add903/emu2413.c): audited MIT adaptation and reference traces.
