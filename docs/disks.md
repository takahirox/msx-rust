# Floppy disks and interface profiles

Issue #29 adds one drive A: and the **Weave WD2793** interface profile on MSX1
and MSX2. This deliberately small boot environment runs Z80 firmware and programs
loaded from disk. It does not provide MSX-DOS, a FAT filesystem, Disk BASIC, or
universal compatibility with disk games. C-BIOS alone has no disk firmware. Issue #41 adds a separately supplied
**Philips NMS1200-style** external interface, described below.

## Weave browser workflow and ownership

The page still starts in MSX2 Weave BASIC. Download the original bootable demo,
choose **Open DSK**, then **Boot disk**. Focus the screen and press Space. The
program displays `WEAVE DISK BOOT` and `Z80 LOADED FROM DRIVE A`. Media initially
mounts read-only; Space reports `WRITE PROTECTED`. Uncheck **Read-only**, focus
again and press Space: `SAVED` appears and byte zero of logical sector 18 increments.
Release Space between writes. **Export modified DSK** downloads a complete private
image, with identical geometry and all other bytes preserved. The demo includes
original source, text and glyphs, and is MIT licensed.

Mounting validates and stages the entire image without resetting execution or
changing the model, firmware, cartridge, SRAM, or previous disk on failure. Weave firmware
and its slot ROM are prepared with the machine, embedded in Wasm; no firmware fetch
is needed at mount/boot. An import generation token and synchronous Wasm commits
prevent stale disk/ROM/firmware imports from changing a newer session. Eject is a
cancellation boundary; other controls are disabled while importing.

Boot disk explicitly gives the disk priority, installing the original reset loader
in slot 0 and disk ROM in slot 2. The cartridge remains parked in slot 1, including
SRAM. Open ROM deliberately gives the cartridge priority and restores automatic
C-BIOS. Explicit C-BIOS/local/diagnostic firmware is never silently replaced by
Boot disk: choose **Use Weave BASIC** first. That action explicitly clears firmware
and cartridge selections, preserves mounted media, and returns to BASIC. Changing
model keeps the disk but follows the existing behavior of clearing incompatible
firmware/cartridge selections and starting BASIC. Reset retains the mounted disk,
write protection and modified bytes, and resets the controller and CPU.

Dirty images must be exported before Eject disk or replacement; these actions fail
without changing anything otherwise. Export marks the downloaded snapshot clean;
further writes mark it dirty again. Eject from disk mode restores BASIC, or the
parked cartridge's C-BIOS if present. Ejecting a mounted disk while running another
firmware retains that firmware. A clean in-progress controller transfer is aborted
on media change; an incomplete sector write never commits. Disk images exist only
in this page's memory: no uploads, analytics, service-worker storage, disk-image
localStorage, or server-side file handling. A browser leave warning covers unsaved
changes, but export before closing/reloading the page to keep them. SRAM retains
its existing independent local save workflow. Export does not overwrite a local
source file; the browser chooses the download destination.

## Raw media and geometry

Only exactly 368,640-byte (360 KiB, 80 cylinders, one side) and 737,280-byte
(720 KiB, 80 cylinders, two sides) raw images are accepted. Both use nine numbered
sectors 1..9 per track and 512 bytes per sector. Sides are interleaved per cylinder:

```text
offset = ((cylinder * sides + side) * 9 + sector - 1) * 512
```

Cylinder must be 0..79, side 0..sides-1. Length determines geometry; raw images have
no required header or checksum, and arbitrary data/unformatted sectors are legal.
No BPB or filename can override the geometry. Empty, truncated and other lengths
are rejected before state changes. DMK/FDI, 40-track disks, variable sector sizes,
weak bits, CRC anomalies and copy-protected/nonstandard tracks are unsupported.
A standard raw DSK cannot encode arbitrary physical track/protection information.
The generated demo is a raw boot disk, not a DOS-formatted FAT disk.

## Philips NMS1200-style external interface

In **ROMs & advanced system options → Disk interface**, choose **Install Philips
disk ROM** and select a local `.rom`/`.bin` of exactly 16,384 bytes beginning `AB`.
The UI explicitly names this profile; a header/size check does not establish ROM
compatibility. It is a single-drive, upstream-backed profile, not an independently
verified physical replica. No Philips ROM is bundled or downloaded.

Installation reserves the complete upload before committing ownership in the core.
The core validates before any state change. Installation then resets execution,
retaining selected main BIOS, MSX2 sub-ROM, boot policy, cartridge/SRAM, media,
protection and dirty bytes. Failed validation/reservation/read, native picker
cancellation and stale asynchronous results leave the running session unchanged.
The shared import generation and synchronous Wasm commits prevent cross-import
races. Cancel import preserves the running session and installed interface; media/cartridge
Eject also invalidates pending imports. An
active Weave reset loader is incompatible with this wiring, so installation is
rejected until Use Weave BASIC or Open ROM returns to system firmware.

With this interface installed, Boot disk requires mounted media and restarts the
**selected system firmware**. It never substitutes a main BIOS or imposes the
Weave `WV` signature. The supplied system firmware must discover/initialize the
AB disk ROM and provide any needed BIOS/OS services. Installing an interface does
not add Disk BASIC or MSX-DOS. C-BIOS alone is not a complete disk environment.

| Action | Philips interface and media | System firmware / cartridge |
| --- | --- | --- |
| Reset / Boot disk | Retain ROM/media, abort partial transfer, reset FDC/latches | Retain BIOS/sub-ROM, cartridge/SRAM; restart CPU |
| Eject disk | Retain interface; reject dirty eject, otherwise abort transfer and signal change | Retain firmware, cartridge/SRAM; browser resumes session |
| Install/replace disk ROM | Validate/reserve first, reset FDC, retain media including dirty bytes | Retain firmware, cartridge/SRAM; restart CPU |
| Remove interface | Abort transfer, remove ROM/FDC decode, retain media including dirty bytes | Retain firmware, cartridge/SRAM; restart CPU |
| Firmware / cartridge switch | Retain interface and mounted media, reset FDC | Existing explicit/automatic firmware and SRAM persistence rules apply |
| Machine switch | Retain interface and mounted media, reset FDC | Existing model controls clear incompatible system/cartridge selections |
| Use Weave BASIC | Retain interface and media | Clear explicit system firmware and cartridge; start automatic BASIC |

The ROM is read-only in **primary slot 2, secondary 0, 4000h–7FFFh**, with the
following eight-byte overlay. No other pages or slots decode registers or
acknowledge IRQ, DRQ or media change. MSX2 FFFFh expanded-slot precedence, slot
3-0 sub-ROM, and slot 3-3 mapper RAM continue to work normally. Interface IRQ is
polled and remains disconnected from CPU IRQ.

| Address | Read | Write |
| --- | --- | --- |
| 7FF8h | WD status, acknowledges IRQ | WD command |
| 7FF9h | Track | Track (idle only) |
| 7FFAh | Sector | Sector (idle only) |
| 7FFBh | Data, consumes a read byte when DRQ | Data, supplies a write byte when DRQ |
| 7FFCh | Full side latch | Full latch retained; bit 0 selects side |
| 7FFDh | Drive latch with bit 2 replaced by active-low media-change | Bits 1:0: 00/10 A:, 01 absent B:, 11 no drive; bit 7 motor |
| 7FFEh | FFh; no Weave media/geometry extension | Ignored |
| 7FFFh | Bit 6 **active-low IRQ**, bit 7 **active-low DRQ**, all other bits pulled high | Ignored |

Signal reads do not acknowledge either signal. Data transfer services DRQ; status
reads acknowledge IRQ. Drive readback reports bit 2 low once after A: insert/eject,
then high; it acknowledges change only for selected A:, even with the motor off.
Absent B:/no drive behave as unchanged upstream dummy drives and do not consume
A:'s change. Reset sets side/drive latches to 00h (A selected, motor off) and
clears IRQ/DRQ, retaining media/change/dirty/protect state. Other drive-latch bits
read back; bit 2 is always replaced by change status. Only effective side,
drive or motor changes abort an in-progress sector. Equivalent A selectors and
unused latch bits do not abort it. Controller timing and command limitations below
apply to both profiles; motor spin-up, drive mechanics and flux timing remain
approximations.

Behavioral evidence is pinned to openMSX commit
`26e1e4945df406e250a05122889f88d40edd828b`:
[NMS1200 configuration](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/share/extensions/Philips_NMS_1200.xml),
[Philips register implementation](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/fdc/PhilipsFDC.cc),
and [drive change contract/dummy drive](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/fdc/DiskDrive.cc).
The generic upstream decoder has aliases; its selected external configuration
maps **only page 1**, which is the boundary implemented here. Upstream explicitly
leaves side/drive unused-bit readback and the applicability of change bit 2 to
other physical machines uncertain. We deliberately select that latch/readback
policy, including pull-ups at 7FFFh. The Philips service manual was inaccessible
during planning. No independent physical-hardware verification is claimed.
Implementation and fixtures are independently authored from these behavioral
observations; no upstream implementation or ROM bytes are copied.

## Weave slots and exact controller wiring

This is an original MSX-compatible memory-mapped profile, not a claim to duplicate
a particular manufacturer's complete interface. The ROM and wiring must be used
together. Only while original disk firmware is active, primary slot **2**, secondary
0, exposes a 16 KiB AB-header ROM at 4000h..7FFFh; its last eight bytes are hardware
registers. Other slots cannot access the FDC. Slot 1 cartridge bytes/banks are never
overwritten. The reset loader maps page 0 to slot 0, page 1 to slot 2, pages 2/3 to
RAM in slot 3 (secondary 3 on MSX2). The MSX2 secondary register remains functional;
no C-BIOS sub-ROM is installed in this original boot mode. No mapper RAM is added.

| Address | Read | Write |
| --- | --- | --- |
| 7FF8h | WD status (acknowledges IRQ) | WD command |
| 7FF9h | Track register | Track register (idle only) |
| 7FFAh | Sector register | Sector register (idle only) |
| 7FFBh | Data register / consume read byte when DRQ | Data register / supply write byte when DRQ |
| 7FFCh | Side latch | Bit 0: side |
| 7FFDh | Drive/motor latch | Bits 1:0 = 01 selects A:, bit 7 motor on; other values not ready |
| 7FFEh | Profile media extension: bit 7 present, bit 0 two-sided, bit 1 changed; read clears changed | Ignored |
| 7FFFh | Bit 7 IRQ, bit 6 DRQ (active high) | Ignored |

IRQ is polled through the interface; it is **not connected to the CPU interrupt
line**, which remains the VDP IRQ. Reading status clears IRQ, reading the signal
latch does not. Insert/eject and completion raise interface IRQ; controller Reset
clears it, cancels transfers, restores track/side zero, sector one, motor off, while
retaining media/change/dirty/protect state. Disk change clears only by 7FFEh read.

Supported WD command families: restore, seek, step, step-in, step-out (track update
bit honored); single/multiple read and write sector; force interrupt (immediate,
ready transitions and index condition). Busy ignores new commands except force
interrupt. The physical head is bounded to 0..79 and is separate from the Track
register. Each issued SEEK pulse moves the head and updates Track by one toward
the latched Data target; equal registers cause no movement and existing
head/register offsets persist. STEP retains the last direction; STEP-IN/OUT set
it. The T bit controls STEP-family register updates independently of head motion.
RESTORE sets Track=FFh and Data=0, steps outward following the track-zero sensor,
then repairs Track=0. An outward attempt when already at zero suppresses the pulse,
repairs Track=0 even with T clear, and completes immediately. Register arithmetic
otherwise remains eight-bit, independently of the head.

At the drive's upper stop, an attempted inward pulse performs any requested Track
update but keeps the head at 79 and immediately reports seek error, regardless of
V. Earlier movement is retained; an out-of-range or mismatched Track cannot alias
an image sector. A later RESTORE can recover the head/register correspondence.
This hard upper stop/error is the bounded raw-image drive's policy, not a claim
about physical drives' extra cylinders. Type I commands with V set verify the
resulting Track against the physical head and mounted track/side geometry,
reporting seek error on a mismatch; V clear skips
that check. Side compare is honored when enabled. Multi-record transfers increment
the sector register and terminate with record-not-found after sector 9 rather than
silently crossing a track. Unsupported read-address/read-track/write-track/format
commands return record-not-found. CRC/deleted marks and density controls are outside
this raw-sector subset.

Status: bit 0 busy, bit 1 DRQ for sector commands, bit 2 lost data, bit 4 seek/
record-not-found, bit 6 write-protect failure, bit 7 not ready. Type I status uses
bit 2 for track zero, bit 1 for index, bit 6 for the media protect signal instead.
Status errors persist until the next command. A drive is ready only with media,
A: selected and motor on. Writes check protection before and after transfer.

Type-I movement uses the 1 MHz WD2793 profile: rate bits 00/01/10/11 select
6/12/20/30 ms **per pulse**. At the 3,579,545 Hz CPU clock, event `n` is observed at
`ceil(n × rate_ms × 3,579,545 / 1000)` elapsed cycles. Fractional-cycle deadlines
are retained across pulses, so rounding never accumulates. Every elapsed movement
event is consumed even when a single `tick()` spans multiple pulses; large,
irregular and single-cycle partitions give the same movement and completion.

The first eligible pulse occurs at command start, subsequent pulses at the chosen
interval, and completion after the final interval. Thus a synchronized 0→79 SEEK
at rate 00 issues pulses at 0, 6, …, 468 ms and completes at 474 ms (1,696,705 CPU
cycles). Equal Track/Data with V=0 completes immediately with no artificial delay.
BUSY stays asserted and DRQ clear until completion, when interface IRQ rises.
FORCE INTERRUPT cancels future events but retains all issued head/register changes;
status/signal acknowledgement and polarity are unchanged. Side/drive/motor abort,
media replacement/ejection, reset and interface removal cancel pending events;
reset/removal retain their existing reset-to-zero lifecycle.

The first-pulse/final-interval convention follows the
[pinned openMSX scheduling reference](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/fdc/WD2793.cc)
(`startType1Cmd`, `seek`, `step`, `seekNext`), independently implemented here.
The rate table and movement/register/zero-sensor behavior are supported by the
[TI second-source TMS279X/WD279X manual](https://deramp.com/swtpc.com/DC_5/TMS279X_DataSheet.pdf),
printed pages 12–13 and 17, an edited transcription without physical-device
confirmation. The upstream verification sequence itself has a TODO. We retain
our existing verification-result predicate after movement; verification settling
and rotational ID search, READY-independent Type-I execution, head-load fidelity
and exact internal startup latency remain separate gaps. In particular this
profile still requires READY for Type I and omits internal command startup delay.
CPU accesses and ticks remain instruction-boundary events (see
[CPU timing](architecture.md#cpu-fetch-timing)).

Sector-transfer timing is unchanged: DRQ is first raised after 114 CPU cycles,
then 114 cycles after each serviced byte. An unserviced DRQ aborts with lost-data
after 4,096 cycles. This generous deterministic service window is an approximation,
not rotational/flux accuracy. Index repeats every 715,909 cycles (300 rpm), high
for 7,000 cycles.
Sector data is bounded to a fixed 512-byte transfer buffer. Writes commit only
once all 512 bytes arrive, retaining all bytes outside that sector. A canceled,
protected or timed-out partial write leaves the mounted image unchanged.

## Weave firmware ABI and boot contract

The source generator is [disk.rs](../crates/msx-core/src/firmware/disk.rs); the
hardware implementation is [disk.rs](../crates/msx-core/src/disk.rs). Everything
below runs as Z80 instructions and ordinary memory/VDP/PPI bus accesses. There are
no Rust BIOS traps, host disk shortcuts, or browser-rendered program output.

The reset loader selects the slots and jumps to ROM initialization at 4100h.
Initialization calls DISKIO to read logical sector 0 to C000h. Bytes 510/511 must
be ASCII `WV`, our explicit boot contract; otherwise the CPU halts with a red
backdrop and the browser reports the unsupported boot contract. The firmware then jumps to C000h. This signature and entry are **not**
a generic MSX-DOS boot-sector contract. Appropriate software must use this boot
contract or run against the documented interface itself. The demo boot sector
calls DISKIO to load logical sectors 1..10 into C200h..D5FFh and jumps to C200h.
It initializes video, copies its disk-loaded glyphs/text to VRAM, polls the keyboard
matrix, and uses DISKIO to read/increment/write LBA 18 via an E000h buffer.

Implemented standard disk-driver entry points (slot 2 must be visible):

- **4010h DISKIO**: A=0, B=sector count (1..255), C=F8h/F9h, DE=zero-based logical
  sector, HL=RAM buffer, carry clear/read or set/write. Geometry comes from C
  (F8h single side, F9h double side); caller must match mounted geometry. Each
  sector's buffer must start within 8000h..ED00h, with RAM mapped on pages 2/3.
  Returns B=completed sectors, carry clear on success; carry set with A=0 protect,
  2 not ready, 8 seek/record/transfer failure, or 12 parameter error. AF/BC/DE/HL
  may change. F100h..F104h are driver scratch/boot status; stack starts at F000h (EF00h..EFFFh reserved for stack).
- **4013h DSKCHG**: A=0, carry clear and B=FFh changed or 1 unchanged; carry set,
  A=2 for absent/invalid drive. Uses the controller's media-change latch.
- **4019h CHOICE**: HL=0 (no formatting choices).
- **401Fh DSKSTP**: stops the drive motor.
- **4016h GETDPB** and **401Ch DSKFMT** return carry set/A=12 unsupported. The demo
  does not need these services. No filesystem, BDOS, CALSLT, Disk BASIC extension,
  DOS kernel or full main BIOS API is supplied by this firmware.

External MSX disk programs may require proprietary or separately licensed firmware,
DOS/BASIC services, a different controller wiring, mapper RAM, or copy-protection
features. No commercial or arbitrary external disk-game compatibility is claimed.
Supply any additional firmware/OS only with lawful permission; none is bundled.

## Original Philips fixtures

[philips.py](../tests/fixtures/philips.py) generates the original, MIT-licensed
sparse hex system/disk-ROM/sector fixtures used by native and compiled-Wasm tests.
Regenerate with `python3 tests/fixtures/philips.py`. They discover the AB header and
call its initialization vector through normal slot mapping. The disk ROM polls
active-low signals, reads a sector without `WV` into RAM, then executes its code.
The sector renders a green VDP backdrop and polls a complete 512-byte A6h write to
sector 9 (side 0 for 360 KiB, side 1 for 720 KiB). Native variants test protection, forced abort and service timeout;
expected full images prove exact writes and unchanged partial writes. The system
also accesses a parked ASCII8 SRAM cartridge, the MSX2 sub-ROM and separate mapper
RAM segments. No host BIOS traps or proprietary inputs are involved. Tests cover
both geometries/models and the browser suite runs at `/` and `/msx-rust/`.
The separate original `philips-movement.hex` sector probe (#47) observes an
intermediate Track=2 and BUSY during a five-track seek, forces an immediate IRQ,
checks active-low signals/acknowledgement, waits beyond the original seek duration,
then reads track 2/sector 1 and writes its eight-byte observation trace to track
2/sector 9. Native tests and compiled-Wasm browser tests execute the same bytes
on both models; green VDP output and the exact exported image require every CPU
check to succeed. Existing generated fixtures remain byte-for-byte unchanged.
These fixtures establish the selected bus contract, not commercial compatibility.

## Provenance and reproducibility

The drive, controller, reset loader, Weave disk ROM, demo program, synthetic sector data,
text and original font are independently authored under this repository's MIT
license. No proprietary disk BIOS, MSX-DOS files, Microsoft BASIC, commercial disk,
third-party routines or copied ROM bytes are inputs. The separately bundled
BSD-licensed C-BIOS remains a cartridge BIOS. Local Philips-compatible ROMs remain
private user inputs and are not included in assets or tests. Retain LICENSE
and [ROM-LICENSE.txt](../firmware/ROM-LICENSE.txt) when redistributing assets.

Build with `./scripts/build.sh`, or generate standalone assets with:

```sh
cargo run --locked -p msx-core --example assets -- OUTPUT_DIRECTORY
```

This emits `weave-disk.rom` (16 KiB) and `weave-disk-demo.dsk` (720 KiB).
The ROM is also embedded from the same generator in Wasm. No assembler, downloaded
ROM, original hardware or proprietary fixture is needed. `check-dist.mjs` checks
asset sizes, signatures, required ABI exports and deployment inventory.

Hardware design reference: Western Digital's WD279X descriptions and command/
status tables in its [1986 Storage Management Products Handbook](https://www.bitsavers.org/components/westernDigital/_dataBooks/1986_Storage_Management_Products_Handbook.pdf).
Only public interface documentation informed the implementation; no firmware
listings or disassemblies were used. The implementation and approximations above
are the authoritative supported profile.

Rust tests cover geometry, offsets, signals/timing, seek/restore, errors, transfers,
protection, exact sector writes, reset/media change, slot visibility and real CPU
boot/write on both models. Browser tests at `/` and `/msx-rust/` verify glyph pixels
in Canvas, keyboard response, exact downloaded bytes, read-only behavior,
Reset/Eject, rejected imports, explicit firmware and import cancellation. The
existing compiled-Wasm regression suites cover C-BIOS, local firmware, demos,
mappers/SRAM and SCC audio alongside these tests.

Post-merge verification required by #29 remains pending: record the deployment
commit, successful Pages Actions run, actual published URL and observed MSX2 BASIC,
disk boot/input/export byte comparisons and safe return to BASIC. Local tests do
not establish published-site behavior. Do not push/deploy solely to perform this
pre-merge implementation task.

## Implementation validation — 2026-10-21

Final local results for Issue #29:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed, 112 tests including 8 disk tests and allocation-failure/commit checks |
| `./scripts/build.sh` | Passed with pinned Rust 1.99.0 and the local linker workaround below |
| `node scripts/check-dist.mjs` | Passed, including disk assets and Wasm ABI |
| `npm test` | Passed, all 82 tests (8.1 minutes) |
| `git diff --check` | Passed |

Compiled-Wasm tests at both roots observed the original demo's glyph pixels,
Space-triggered read-only error and successful write, and compared exported images
byte-for-byte: only byte 9216 (LBA 18 byte zero) changed from 0 to 1. They verified
Reset retains dirty bytes, dirty replacement/ejection is rejected, export enables
safe ejection to BASIC, explicit/local firmware survives rejected boot attempts,
failed/canceled imports retain the prior session, and disk boot parks cartridge
SRAM safely. A separate original 360 KiB Rust fixture executes Z80 reads and writes
at the final sector on both models. Screenshots were generated as
`test-results/disk-msx{1,2}-{root,pages}.png`; CI can regenerate and retain them with
its existing browser evidence artifact. Native/unavailable/rejected fullscreen
API cases also pass in short landscape viewports.

Local environment note: Homebrew Rust shadowed rustup, so verification selected
`$(rustup run 1.99.0 rustc --print sysroot)/bin` first in PATH, as documented in
README. This host's pinned macOS `rust-lld` additionally could not resolve
`libLLVM.dylib`. A task-local, ignored linker wrapper set `DYLD_LIBRARY_PATH` to
the pinned toolchain's `lib` directory and executed that same toolchain's
`lib/rustlib/aarch64-apple-darwin/bin/rust-lld` with unchanged arguments;
`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER` selected the wrapper. No repository
toolchain/build configuration, compiler version, or external system files were
changed to work around this host setup. Normal installations/CI use the unchanged
build command. These are local pre-merge results; the published-site check above
remains pending after merge and deployment.


## Issue #41 validation (2026-10-24)

The final implementation passed these local checks with pinned Rust 1.99.0:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed: 156 tests, including seven new native/Wasm cases |
| `./scripts/build.sh` | Passed with the local linker wrapper described above |
| `node scripts/check-dist.mjs` | Passed: inventory, notices, new ABI exports and import-free Wasm |
| `npm test` | Passed: 98 browser cases, including ten Philips cases at both roots |
| `git diff --check` and staged whitespace review | Passed |
| Fixture regeneration | Passed: generated hex unchanged |

Regression sensitivity was checked by temporarily restoring the baseline firmware
visibility gate, then separately routing the new interface through Weave wiring.
Each mutation failed three of the five new core regressions, including the
CPU-executed fixture; restoring the implementation passed all five again. These
are behavioral mutation checks, not a claim that the new APIs exist on the
baseline. All 88 existing browser cases, including disk, MSX1/MSX2, PSG/SCC audio,
firmware and SRAM coverage, remain in the passing full suite.

Initial attempts failed because this host's pinned rust-lld could not locate
libLLVM.dylib, and the first browser fixtures assumed the MSX1 palette on MSX2 and
did not await a preceding import. The same pinned linker succeeded with the
ignored task-local wrapper documented above; corrected fixtures and final full
runs passed. One distribution check ran before the updated notice was rebuilt;
rebuilding synchronized it and the final distribution check passed.

No required automated check was skipped or left unperformed. Optional physical
hardware/trace comparison and proprietary firmware/software boot testing were not
performed; no commercial compatibility is inferred. Coordinator review remains a
separate pre-merge step. Issue #41 requires no post-merge verification. Nothing was
pushed, published, merged or commented during this implementation.
