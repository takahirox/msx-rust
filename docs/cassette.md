# Standard cassette playback

Issue #63 adds read-only MSX CAS playback to both models. Open CAS mounts local
bytes without resetting the CPU, selecting firmware, injecting a load command or
writing decoded data into RAM. The supplied Z80 firmware/software must poll PSG
R14 bit 7. In landscape fullscreen the cassette/disk controls share a horizontally scrolling
strip; keyboard focus brings each control into view.
Choose 1200 baud (default) or 2400 baud before importing; changing the
picker applies to the next import. Rewind resets only tape position and fractional
phase. Eject releases the medium. Images are private, transient page memory:
no network upload, storage, IndexedDB or local file modification is involved.

Pause and background-tab suspension stop CPU stepping and thus tape time. The
software controls the active-low PPI C4 motor through AAh or ABh bit-set/reset.
Motor-off freezes position and fractional phase and forces input low. Resuming
continues the same pulse. End-of-tape holds at the end, reads low and never wraps;
status distinguishes end-of-tape from the independent motor latch. Reset retains
bytes, rate and position and reapplies the existing PPI reset latch C=00h, so the
motor becomes enabled. Firmware, cartridge and model transitions use that same
reset policy; they neither eject nor rewind. Use Rewind followed by Reset to
restart the bundled diagnostic. Tape-only controls preserve pause, CPU, firmware,
cartridge banks/SRAM, disk/interface state, BASIC and audio state.

C-BIOS has no original MSX BASIC; Weave BASIC does not implement cassette commands.
This hardware capability does not turn either into a complete BASIC tape-loading
environment. Compatible system firmware and software must be supplied separately.
Recording, WAV, TSX, exotic/custom waveform loaders, analog degradation and audible
tape monitoring are outside scope. Diagnostic success is not evidence that a
commercial tape or genuine BIOS tape loader works; none has been observed here.

## Raw Wasm interface

`msx_tape_upload(length)` reserves bounded staging memory and returns its pointer,
or zero on failure. Reacquire `memory.buffer` after reserving, fill exactly those
bytes, then call `msx_mount_tape(length, rate)` with rate 1200 or 2400. Mount consumes
the shared staging buffer and returns 1 on success or 0 on failure; read the
existing `msx_error()` / `msx_error_len()` message immediately after a failure.
`msx_cancel_upload()` discards an uncommitted transfer. `msx_rewind_tape()` and
`msx_eject_tape()` change only the transport. `msx_tape_flags()` reports mounted
(bit 0), PPI motor enabled (bit 1), and end-of-tape (bit 2); the motor latch can
remain enabled with no medium or at the end. `msx_tape_rate()` returns the last
successfully mounted rate, initially 1200. `msx_tape_position()` returns elapsed tape seconds
as f64 (zero without media), based on emulated CPU cycles rather than wall time.

## Reconstruction policy and limits

MSX CAS is a byte format with an unescaped eight-byte block marker:
`1F A6 DE BA CC 13 7D 74`. Markers separate blocks and are never transmitted. We
require the first marker at offset zero and nonempty payloads after every marker.
A full marker anywhere in data necessarily becomes a boundary: CAS cannot tell an
embedded marker from an actual boundary. Partial marker prefixes at EOF are data.
CAS does not preserve waveform, speed, polarity or exact gaps, and cannot reliably
distinguish accidental type signatures from real headers. These ambiguities are
intrinsic to this format, not resolved through title heuristics.

At a file boundary ten D0h (binary), D3h (tokenized BASIC), or EAh (ASCII) bytes
identify a standard header. At least six filename bytes must follow; additional
header bytes are preserved, not discarded. Binary and BASIC headers require a
following body. ASCII bodies continue until a block containing 1Ah, including
all bytes after that terminator; a missing body/terminator is rejected. A type-like
prefix inside an expected body remains body data. Multiple complete files are
supported. Other nonempty blocks are preserved byte-for-byte as unknown data.
Each unknown block receives a conservative long leader and long gap; this may
not reproduce a custom loader's original timing. We do not reinterpret binary
addresses, tokenized BASIC or ASCII text, repair headers, or truncate payloads.

Each long leader is preceded by two seconds of idle-low silence; each short
leader by one second. Standard headers and unknown blocks use long leaders;
standard bodies and ASCII continuations use short leaders. Initial signal
polarity is high at the beginning of each leader and framed bit. A zero has
one low-frequency cycle (high for two quarter-bit units, low for two); a one
has two high-frequency cycles (high/low/high/low). Framing is one zero start,
eight LSB-first data bits and two one stop bits. At 1200 baud frequencies are
1200/2400 Hz and long/short leaders contain 16000/4000 high-frequency cycles.
At 2400 baud frequencies and leader cycle counts double, retaining leader and
gap durations. There is no extra trailing gap: the final stop pulse completes,
then the medium ends low.

The 3,579,545-Hz CPU clock advances a rational position by `cycles * 4 * baud`;
one quarter-bit unit is exactly 3,579,545 position units. Fractional phase is
retained across instructions, motor stops and segment boundaries. Sampling and
motor writes follow the core's existing instruction-boundary peripheral timing,
not a new sub-instruction bus scheduler. Blocks store byte ranges and leader/data
boundaries; waveform edges are computed when sampled, without expanding pulses.
Images are limited to 16 MiB and 65536 blocks. Both limits bound parsing and
metadata; arithmetic remains within u64 at either rate. Allocation, format, size,
invalid-rate, canceled-stage and length failures preserve the previous transport
and session. The browser's generation token also rejects stale asynchronous reads.

R14 bits 0–5 keep both joystick masks/common lines, bit 6 stays high, and bit 7
alone carries cassette input. Existing fixed R14-input/R15-output direction policy,
R7/audio controls, keyboard row selection and other PPI bits are unchanged.

## References and original fixtures

The [MSX2 Technical Handbook, chapter 5, sections 2.1–2.5](https://raw.githubusercontent.com/Konamiman/MSX2-Technical-Handbook/master/md/Chapter5a.md)
describes standard framing, frequencies and leaders. Its prose naming R15 as the
input conflicts with its R14 example. Pinned openMSX
[MSXPSG](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/sound/MSXPSG.cc)
and [MSXPPI](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/MSXPPI.cc)
corroborate R14.7 and active-low C4. Its
[CasImage](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/cassette/CasImage.cc)
and [CassettePlayer](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/cassette/CassettePlayer.cc)
provide behavioral references for markers, leaders and motor-controlled position.
We use standard rates instead of upstream's accelerated 3744-baud policy. No
upstream code or ROM bytes were copied and no upstream runtime comparison was run.

`crates/msx-core/src/firmware/cassette_fixture.rs` independently authors the MIT
Z80 cartridge and CAS generated as `dist/cassette-test-cartridge.rom` and
`dist/cassette-test.cas` by the existing assets example/build script. They contain
only original instructions and synthetic data. The cartridge boots under unmodified
bundled C-BIOS on either model, then directly uses PSG/PPI/VDP ports. It measures
high pulses to calibrate the decoder, counts both leaders, decodes a 16-byte
binary header and all 34 body bytes into C200h–C231h, stops/resumes the motor
inside the last header stop pulse, and jumps to tape-loaded code at C216h. That
code makes the backdrop green, writes A5h to C1FFh and stops the motor. Measured
leader counts are at C180h/C182h; C184h records a pulse width and C185h records
idle-low during the motor stop. All 50 recovered bytes, including addresses,
executable instructions and 00/FF/55/AA/80/81/FE/7F terminal data, appear as four
bytes per screen row: each byte has eight LSB-first tiles, white=1, green=0 after
success. No decoded bytes are supplied by tests or native BIOS services.

Native expectations derive edge timestamps independently using explicit
clock/rate arithmetic and literal framing vectors. Tests compare single-cycle,
irregular and large partitions, terminal bytes, known/unknown blocks, gaps,
long playback, malformed imports, allocation failure, motor phase, reset and
transport lifecycle. Browser tests use real file choosers and compiled Wasm at
both `/` and `/msx-rust/`, independently check every recovered byte in Canvas,
and cover pause, rewind, eject, bad transfers/rates/allocation and stale imports.
See [recorded validation](validation.md#issue-63-cassette-playback).
