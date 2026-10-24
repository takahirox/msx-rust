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
repairs Track=0 even with T clear, and completes movement immediately (verification
still follows if V=1). Register arithmetic otherwise remains eight-bit,
independently of the head.

At the drive's upper stop, an attempted inward pulse performs any requested Track
update but keeps the head at 79 and immediately reports seek error, regardless of
V. Earlier movement is retained; an out-of-range or mismatched Track cannot alias
an image sector. A later RESTORE can recover the head/register correspondence.
This hard upper stop/error is the bounded raw-image drive's policy, not a claim
about physical drives' extra cylinders. Type I commands with V set settle and
search matching-cylinder IDs as specified below; V clear skips verification.
Sector side compare is honored when enabled. Multi-record transfers increment
the sector register and search again; sector 10 fails after its fifth observed
index without crossing a track. Unsupported read-address/read-track/write-track/format
commands return record-not-found. CRC/deleted marks and density controls are outside
this raw-sector subset.

Status: bit 0 busy, bit 1 DRQ for sector commands, bit 2 lost data, bit 4 seek/
record-not-found, bit 6 write-protect failure, bit 7 not ready. Type I status uses
bit 2 for track zero, bit 1 for index, bit 6 for the media protect signal instead.
Status errors persist until the next command; bit 7 always reflects live READY.
A drive is ready only with media, A: selected and motor on. Writes check protection before and after transfer.

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
status/signal acknowledgement and polarity are unchanged. Media replacement/ejection,
reset and interface removal cancel pending events; reset/removal retain their
existing reset-to-zero lifecycle.

The first-pulse/final-interval convention follows the
[pinned openMSX scheduling reference](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/fdc/WD2793.cc)
(`startType1Cmd`, `seek`, `step`, `seekNext`), independently implemented here.
The rate table and movement/register/zero-sensor behavior are supported by the
[TI second-source TMS279X/WD279X manual](https://deramp.com/swtpc.com/DC_5/TMS279X_DataSheet.pdf),
printed pages 12–13 and 17, an edited transcription without physical-device
confirmation. The upstream verification sequence itself has a TODO and is not a
verification oracle. Type I executes independently of READY: diskless and
motor-off A can position normally. READY remains live status bit 7, separate from seek error.
Side/drive/motor writes do not cancel Type I; subsequent pulses and feedback use
the newly selected drive. Only installed A receives physical steps. Absent B and
no-drive selections still advance controller registers but expose no A zero,
index or write-protect signals. RESTORE initializes Track to FF and fails with
seek error after 255 unsuccessful pulses plus the final rate interval if there
is no zero feedback. A's track-zero signal remains motor/media independent in
this selected model, not a claim about universal physical wiring. Philips reset
latch 00 selects A with motor off; Weave reset selects no drive.

V=0 retains the movement completion timing above. V=1 (also for equal-register
SEEK, already-zero RESTORE and outward STEP at zero) adds exactly 30 ms of settling,
then searches IDs with **HLT assumed high**. BUSY stays set, DRQ stays clear, and
IRQ rises only on success or failure. Fractional movement and settling deadlines
are retained on one clock; all elapsed movement, settling, ID and index events
are consumed chronologically across large ticks.

Raw DSK carries payloads, not ID/gap/CRC timing. Our normal-track approximation
synthesizes nine good-CRC IDs on every supported cylinder/side: cylinder is the
physical head position, side is the selected side, sectors 1..9, size 512 bytes.
The global rotational phase starts at zero on reset and advances even while the
motor is off. Its period is 715,909 CPU cycles (300 rpm); motor enable/selection
does not reset phase, and physical spin-up is omitted. ID `k+1`, `k=0..8`, occurs
at `ceil(k × 715909 / 9)` cycles within each revolution:

```text
ID:       1      2       3       4       5       6       7       8       9
phase:    0  79546  159091  238637  318182  397728  477273  556819  636364
```

Rising index coincides with ID 1 at phase zero (status index stays high for 7,000
cycles). At the same deadline: finish movement, finish settling, inspect ID, then
count rising index. Thus an ID at the search-start boundary is eligible, and a
matching ID wins at the fifth-index boundary. Selection/motor/side writes apply
after events already consumed at that clock and never replay an event at the
write's timestamp. For Type-I verification, matching cylinder plus good CRC
succeeds independently of Sector or payload bytes; no CRC faults are invented from payload contents. A
missing side supplies index but no IDs. A cylinder mismatch or missing side fails
on the fifth rising index encountered during search, not after five nominal
wall-clock periods. Only selected A with media and motor on supplies ID/index
input. Without that input the search remains cancellably BUSY, with no fabricated
elapsed revolutions; restoring input resumes at the current global phase. This
no-input policy is a selected-model inference from index-driven failure, not
physical WD2793 confirmation. FORCE INTERRUPT cancels every phase and preserves
completed movement; mount/eject/reset/interface removal remain cancellation
boundaries. Reset/removal keep the existing reset-to-zero policy.

Full HLD/status-bit-5 lifecycle, physical motor spin-up, flux/CRC anomalies,
exact internal startup latency and additional commands remain separate gaps.
CPU accesses and ticks remain instruction-boundary events (see [CPU timing](architecture.md#cpu-fetch-timing)).

Type-II reads/writes have separate settling, rotational search and transfer
phases in both interfaces. READY is checked at command acceptance; absent media,
absent B/no-drive and motor-off commands still fail immediately. A write-protected
write fails immediately before settling; protected reads remain supported. E=1
adds exactly 30 ms (107386.35 CPU cycles), with HLT assumed high; E=0 starts search
without that delay. BUSY stays set and DRQ/IRQ inactive during settling/search.
The fractional deadline is never rounded down onto an earlier ID.

Search reuses the nine-ID/index schedule above. The requested Track must match
the physical-cylinder ID, Sector must match the encountered sector number, and
C=1 compares command S against the ID-side LSB. C=0 ignores S. A missing side has
index but no IDs; payload bytes never invent bad CRCs or missing address marks.
Unmatched cylinder/sector/side searches fail with record-not-found on the fifth
observed rising index. Search-start inclusion and ID-before-index ordering are
shared selected-model policies, not claims of WD2793 silicon confirmation. In
particular an E=0 search at phase zero counts that index, while a search starting
one cycle later must wait for the next index.

The E delay, ID comparisons, index-limited failure and ascending multi-record
searches follow the [TI second-source transcription](https://deramp.com/swtpc.com/DC_5/TMS279X_DataSheet.pdf),
printed pages 10, 12–13, 17 and 20, applying its 1 MHz WD2793 profile. The
[pinned openMSX scheduler](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/fdc/WD2793.cc)
(`startType2Cmd`, `type2Loaded`, `type2Search`, `type2Rotated`) corroborates separate
scheduled settling/search; its optional-side TODO and uncertain stopped-rotation
handling are not behavior oracles. These references are not physical measurements.

After a matching ID, Type-II bytes run independently of CPU service at nominal
250 kbit/s MFM: one byte takes **32 microseconds = 114.54544 CPU cycles** at
3,579,545 Hz. All deadlines share one absolute clock in 1/100000 CPU cycles;
only observation at a CPU instruction boundary rounds upward. Adding the exact
fractional period retains the remainder across every byte and phase. Single,
irregular and large ticks visit the same events in chronological order.

The existing nine ID positions are retained. Their phase denotes the **start of
the FE ID-address-mark byte**, with ID 1 coincident with rising index. Relative to
a matching ID at `H`, let `B = 114.54544 cycles`. The selected raw-image profile is:

| Device event | Absolute deadline | CPU-cycle offset from H (ceil) |
| --- | --- | --- |
| Initial write DRQ | H + 9B | 1031 |
| Initial write check; abort if unserviced | H + 17B | 1948 |
| Second write DRQ (byte index 1) | H + 44B | 5040 |
| Later write request, index i=1..511 | H + (43+i)B | 5040..63459 |
| Consume requested write byte i=1..511 | H + (44+i)B | 5155..63573 |
| First write byte reaches media | H + 45B | 5155 |
| Last write byte reaches media | H + 556B | 63688 |
| Read byte arrival, index i=0..511 | H + (46+i)B | 5270..63802 |
| Final read byte's service budget expires | H + 558B | 63917 |
| Read completion after two CRC bytes; write completion after CRC + trailing FE | H + 559B | 64031 |

The first-to-second write request interval is 35B = 4009.0904 cycles; later
requests are one B apart.

These are **synthetic offsets**, not physical phase measurements. The local normal
MFM layout uses a seven-byte ID header, 22 gap bytes, 12 sync bytes, three A1s and
a data mark, informed by the
[pinned raw-sector layout](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/fdc/SectorBasedDisk.cc).
The eight-byte initial-write window and extended first-to-second DRQ interval
follow the pinned controller's `startWriteSector`/`checkStartWrite`/pre-data
schedule as a selected policy. The first supplied byte is latched at the initial
check; subsequent data uses a one-byte serializer pipeline. The read's extra
byte of assembly latency and its completion boundary are local profile choices.
Raw DSK supplies neither headers nor angular timing. We retain the nine evenly
spaced IDs rather than adopting the upstream track's global gap placement.

On each read arrival, Data updates and DRQ asserts. Replacing unread data latches
LOST DATA and continues. Reading Data acknowledges only the current byte;
repeated/early reads return the register without advancing the disk. The final
byte stays available with DRQ through the CRC tail until serviced or completion.
Missing its one-byte budget latches LOST DATA even without a replacement byte;
a late read during the tail still returns it. Completion clears DRQ, clears BUSY,
and raises IRQ; Data retains its last value. Status reads acknowledge IRQ only.

An initially unserviced write aborts at its separate check with LOST DATA and no
media changes. Once the first byte is latched, later unserviced positions become
zero and LOST DATA latches while the sector continues. A late write supplies the
currently asserted request; it cannot recover an earlier zero-filled position.
Writing Data acknowledges DRQ but leaves the holding register writable: the initial
check and each later consumption sample its current value. Repeated writes before
that deadline replace the current byte without queuing future bytes. Writes after
the initial check cannot replace the latched first byte; writes before the next
DRQ do not service that future request. CPU service never shifts deadlines or
completes/commits byte 512.
LOST DATA survives normal completion, subsequent multi-record searches/transfers,
and their RNF failure; a newly accepted ordinary command or reset clears it.
Existing FORCE INTERRUPT status/latching behavior is unchanged and remains outside
this task. Continued read overrun, initial write rejection, later zero-fill and
the CRC/trailing-FE sequence are informed by the TI transcription, printed pages
7, 11, 20 and 40; the exact final-read service/DRQ policy above is synthetic.

After each complete multi-record sector, Sector increments and a fresh search
starts at the current rotation without repeating E settling. There is no automatic
track crossing: after sector 9, sector 10 remains BUSY until its fifth-index
failure. Earlier complete writes remain committed. FORCE INTERRUPT, reset,
mount/eject and interface removal cancel all pending phases without resurrection.
The existing Type-II policy also cancels on an effective side/control change,
including during settling/search; ignored Philips latch bits and equivalent A
selectors do not cancel. This cancellation is an emulator interface policy,
separate from physical WD2793 drive-switch behavior. Type I retains its existing
non-canceling control-write policy.

Sector data is bounded to a fixed 512-byte transfer buffer. Writes commit only
at the scheduled end of the serializer/CRC tail, retaining all bytes outside that
sector. A completed zero-filled sector commits and becomes dirty if its contents
changed; an explicitly canceled incomplete sector or a protected write is discarded.
Completed earlier sectors survive later cancellation, and dirty-media guards still
require exact export before replacement/ejection. This staged-sector safety policy
deliberately differs from physical partial-write fidelity: a real interrupted write
can leave partial data or CRC damage, which raw DSK does not represent.

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
sector 9 (side 0 for 360 KiB, side 1 for 720 KiB). Native variants test protection, forced abort and later zero-fill;
expected full images prove exact writes, zero-fill and discarded canceled writes. The system
also accesses a parked ASCII8 SRAM cartridge, the MSX2 sub-ROM and separate mapper
RAM segments. No host BIOS traps or proprietary inputs are involved. Tests cover
both geometries/models and the browser suite runs at `/` and `/msx-rust/`.
The separate original `philips-movement.hex` sector probe (#47) observes an
intermediate Track=2 and BUSY during a five-track seek, forces an immediate IRQ,
checks active-low signals/acknowledgement, waits beyond the original seek duration,
then reads track 2/sector 1 and writes its eight-byte observation trace to track
2/sector 9. Native tests and compiled-Wasm browser tests execute the same bytes
on both models; green VDP output and the exact exported image require every CPU
check to succeed. The original `philips-readiness.hex` AB ROM (#53) boots through
the same system fixture without any mounted disk. It observes reset-selected A,
seeks to track 2, polls no-input verification BUSY after more than one second, forces/acknowledges
IRQ, and paints a white checkpoint. The host then inserts a raw image without
resetting CPU/head. Ordinary firmware polling checks delayed no-step verification,
reads retained track 2 and writes `[84,02,80,81,81,BF,80,00]` (hex) to the first
eight bytes of track 2/sector 9. Green VDP output requires success; the exact full
exported image checks unchanged surrounding bytes. Native tests and compiled Wasm
run this diskless-to-mounted sequence on both models at both browser roots.
The original `philips-search.hex` AB ROM (#57) polls Type-II E=0 and E=1 reads
of known sector-3 payloads and delayed sector-10 failure. It checks BUSY and
active-low signals, bounds first-DRQ polls by 30 ms + one revolution + 46 byte periods,
and bounds RNF by four revolutions .. 30 ms + five revolutions. Poll loops include
MSX M1 waits (42 cycles for success, 62 for failure). It writes the fixed trace
`01 FF 7F 00 01 FF BF 10` twice, followed by 496 known 62h bytes, through an E=1
sector-9 write on side 0/1 for 360/720 KiB. Native and compiled-Wasm tests execute
this same ROM on both models/geometries at both browser roots and compare the
entire exported image. Green requires all CPU checks to pass.

Existing 65535-iteration signal polls allow over five million cycles, exceeding
30 ms + five revolutions (3686931.35 cycles). Native Weave boot budgets now allow
600 NTSC frames for twelve reads: each sector is bounded by one revolution plus
559 × 114.54544 cycles, with additional positioning/setup time. The independent
three-transfer/final-track fixture allows 180 frames including a 79-track seek;
a read/write pair allows 120 frames. Browser output waits allow 15 seconds for
these emulated bounds plus host scheduling. Serviced-byte loops fit within the
device byte period and explicitly wait for serializer/CRC completion; first-DRQ count checks occur after the full read so
they cannot cause an artificial overrun. The Weave driver chooses read/write
once per sector to keep both polling paths within that period.

Issue #59 adds original `philips-clock.hex` and `philips-clock-weave.hex`
fixtures from the same generator. The former runs through AB discovery; the
latter boots an original WV sector and loads its diagnostic via real DISKIO.
Both leave an entire read unserviced, verify LOST DATA and the final Data value,
read and compare all 512 independently patterned bytes, reject an initially
starved write without media changes, and let a started write zero-fill all later
positions. A final complete write exports six observed signal/status bytes plus
506 payload bytes. Its prefix is `BF 04 B8 00 04 04` for Philips and
`80 04 B8 00 04 04` for Weave; sector 6 is `61` followed by 511 zeroes, and
sector 5 stays unchanged. Native and compiled-Wasm tests cover both interfaces,
models, geometries and browser roots with full-image comparisons. The diagnostic
poll bound exceeds one revolution + 559 byte periods (779940 cycles); native
execution permits 8M cycles including boot reads and CPU payload verification.

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
