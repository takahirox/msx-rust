# Implementation and validation methodology

## Boundaries

`msx-core` owns the CPU, address bus, VDP, PSG, SCC, firmware, line editor and BASIC.
It is ordinary Rust using the standard library with zero crate dependencies.
`msx-wasm` exports a small C-style ABI for frames, pixels, samples, keyboard events,
ROM staging and errors. JavaScript owns browser APIs only. All ROM generators are
ordinary Rust functions, with no assembler, downloaded artifact, or external ROM
needed to build or boot.

`Machine::step` executes a Z80 instruction and advances VDP (including commands),
PSG, SCC, FDC and, on MSX2, RTC by its elapsed T-states including fetch waits.
`Machine::frame` preserves instruction overshoot between calls and runs
228 × 262 T-states per NTSC frame (MSX2 PAL uses 313 lines), generating roughly 736 audio samples. The VDP renders
active scanlines and raises VBlank after 192 or 212 lines. Interrupt requests remain asserted
until status is read or the interrupt-enable bit is cleared. The CPU acknowledges
IRQ after EI's following instruction; NMI is independently latched. IM 0 uses the
bus-supplied opcode (the MSX bus supplies FFh/RST 38h), IM 1 jumps to 38h, and IM 2
reads its vector through the memory map. RESET reconstructs CPU/peripherals/RAM,
retaining selected ROMs and resetting the native interpreter's program.

In MSX1, slots 0/1/2/3 are respectively system ROM/cartridge/empty/RAM. A8h selects a
slot per 16 KiB page. Empty space reads FFh; RAM accepts memory writes and the
cartridge slot decodes mapper registers. Automatic policy boots Weave BASIC with no
cartridge and installs the model-specific C-BIOS before booting a validated cartridge.
C-BIOS discovers slots itself. The explicit MSX2 diagnostic loader maps pages 1/2 and
calls the AB init pointer. Reset restores mapper power-on banks. See the [mapper table
and selection rules](../README.md#rom-loading-and-legal-guidance). With an external
system ROM, normal reset mapping is restored and that ROM probes slots itself. A9h
reads the active-low keyboard row selected by AAh; ABh handles PPI control and port-C
bit set/reset. PSG registers 14/15 integrate joystick input. Printer and cassette data
are absent. MSX2 expands slot 3 for sub-ROM and RAM; see the [MSX2 hardware, commands,
timing limits and tests](msx2.md).

The Weave BASIC boot ROM's private F0/F1 service interface is specific to Weave BASIC.
This is a native Rust interpreter invoked by Z80 firmware, not an MSX BASIC ROM port.
Both models enable these ports only for active Weave BASIC. Cartridge and
external-BIOS CPU execution are independent of this interpreter. No external BIOS APIs
are trapped, replaced or patched. This distinction is visible in the UI, README,
assembly listing, and license inventory.

### Standard joysticks

Two independent active-low masks store up/down/left/right/trigger A/trigger B in
bits 0–5, initially 3Fh. PSG R15 bit 6 selects connector 1 (0) or 2 (1). R15 bits
4/5 independently drive each connector's pin 8. Standard switches connect their
inputs to this common: low reports held inputs; high reports 3Fh without changing
stored state. R14 bits 6/7 keep the existing 1/0 values. Other R15 bits retain
existing latch/readback behavior. The motherboard compatibility policy fixes R14
to peripheral input and R15 to peripheral output regardless of R7 bits 6/7; R7
readback and bits 0–5 audio mixer controls remain intact. This describes this
emulated motherboard, not universal discrete AY-3-8910 direction/electrical behavior.

Routing follows the [MSX2 Technical Handbook, sections 5.1–5.2](https://raw.githubusercontent.com/Konamiman/MSX2-Technical-Handbook/master/md/Chapter5a.md),
corroborated by pinned openMSX [MSXPSG.cc](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/sound/MSXPSG.cc)
(routing/default direction override) and [MSXJoystick.cc](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/input/MSXJoystick.cc)
(released inputs with high pin 8). These are behavioral references; no physical
measurements were performed. The handbook transcription has errors, and pin-6/7
feedback remains uncertain upstream. Alternate direction wiring, pin-6/7 feedback,
mouse/paddle devices, host Gamepad API, Tab delivery and PPI C7 sound are outside
this implementation.

The original diagnostic stores direct PSG and real C-BIOS call results at
C200h–C20Eh and renders them to SCREEN 1; C210h=A5h marks a finished update.
Before BIOS polling it lowers both commons, because the BIOS calls preserve those
output bits. Native tests compare independent RAM and pixel expectations on both
models under bundled unmodified C-BIOS. Browser tests compare Canvas values via
actual keyboard events at `/` and `/msx-rust/`, including release isolation and
lifecycle boundaries. Window blur and visibility notifications are dispatched by
the browser test harness; they are not OS window-manager measurements. No host
BIOS traps or Weave BASIC services participate.

## CPU tests and vectors

All implementation and tests were independently written for this repository.
No emulator implementation code was copied. The decoder follows the bit fields of the
Zilog opcode tables. The tests in `crates/msx-core/tests/cpu.rs` are executable
vectors, specifying exact instruction bytes, initial state, final state and T-states.
The original vectors below use standalone zero-wait timing.
They are not a claim of exhaustive Z80 conformance.

Representative independently calculated vectors:

| Vector | Initial state / bytes | Expected outcome |
| --- | --- | --- |
| Signed overflow | A=7Fh, `C6 01` | A=80h; S,H,P/V set; C,N,Z clear; 7 T |
| Borrow | A=00h, carry=1, `DE 00` | A=FFh; S,H,N,C set; P/V,Z clear; 7 T |
| Decimal addition | BCD A and operand 00–99; ADD then DAA | BCD sum modulo 100, carry for sum ≥100; all 10,000 pairs |
| Indexed memory | IX=8000h, `DD 36 FE 81`, `DD CB FE 06` | (7FFEh)=03h after rotate; 19 then 23 T |
| Indexed BIT | IY=9000h, (9001h)=80h, `FD CB 01 7E` | Z clear, S,H set, C preserved; 20 T |
| LDIR | HL=2000h, DE=3000h, BC=3, source 1,2,3 | Destination 1,2,3, BC=0, pointers +3; 21,21,16 T |
| EI/HALT/IRQ | `FB 76`, IRQ asserted, IM 1 | EI/HALT execute before acknowledge; return PC=2 on stack; PC=38h; 4,4,13 T |
| IM 2 wrap within vector word | I=80h, vector FFh; [80FFh]=67h, [8100h]=45h | PC=4567h; 19 T |

The arithmetic test exhausts all A/operand pairs for ADD, ADC, SUB, SBC and CP,
including both carry inputs for ADC/SBC. Its flag oracle uses integer range checks
and signed arithmetic independently of the decoder's bitwise overflow calculation.
Other vectors exercise alternate registers, calls/stack, conditional timing, all
prefix families, RRD/RLD, logic parity, INC/DEC, HALT refresh, NMI/RETN and IM 0/1/2.

Not covered by this suite: every possible state for every opcode, undocumented WZ/Q
behavior, silicon-specific flags, access-phase synchronization and contention.
No third-party
ZEX/Fuse corpus or physical-machine differential test has been run. The supplied-ROM
integration example also boots an unmodified local C-BIOS and then checks our
independent cartridge's VDP, keyboard and PSG behavior.

## CPU memory-access order

PUSH, taken CALL, RST and interrupt stack entry write the high byte at the
original SP−1, then the low byte at SP−2, with 16-bit wrapping. EX (SP),HL/IX/IY
reads the low byte at SP and the high byte at SP+1 before either write, then
writes the original register's high byte at SP+1 and low byte at SP. The captured
word becomes the register value; SP and flags are preserved. Ordinary word stores
remain low-byte-first, as do POP/RET reads.

Each byte goes through the current Bus mapping. In particular, writing FFFFh on
MSX2 can change expanded-slot selection before the next access to FFFEh. No CPU
instruction bypasses slot decoding or captures a mapped device for the whole word.
This orders observable accesses within an instruction; peripherals still advance
after the instruction, without access-phase synchronization or contention.

Original `tests/stack_order.rs` recording-bus vectors distinguish instruction-byte
reads from stack/vector accesses and assert complete address/value sequences,
final state, returned cycles and `tstates` increments under both fetch policies.
They cover all PUSH pairs, conditional CALL outcomes, all RST destinations,
NMI/IM0/IM1/IM2, all three exchanges, ordinary HL/IX/IY and ED word stores,
and POP/RET reads, including wrapping addresses. `tests/ram_mapper.rs` executes
original CPU probes that hide/reveal RAM between stack writes, inspect retained
RAM through a mapper alias, and exchange at SP=FFFEh; MSX1 flat RAM is the control.
The [Issue #43 validation record](validation.md#issue-43-stack-and-exchange-access-order)
records actual baseline failures and corrected results.

Zilog UM0080 printed pages 115–118 explicitly establish PUSH sequencing.
Its pages 127–129 describe EX results without explicitly specifying write order.
The pinned openMSX [CPU implementation](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/cpu/CPUCore.cc)
corroborates both paths through `PUSH`, `ex_xsp_SS` and `WR_WORD_rev_slow`.
It was consulted as behavioral evidence; implementation and fixtures here are
independently authored. No reference-emulator trace comparison, physical hardware
measurement or broad commercial compatibility claim follows from these tests.

## CPU fetch timing

`Cpu::new()` and `Cpu::default()` retain zero-wait standalone Z80 timing.
`Cpu::with_fetch_wait(FetchWait::Msx)` adds one T-state for each ordinary M1
opcode fetch, including CB/ED/DD/FD prefixes and each HALT idle fetch. Both
`Machine` models select this policy at construction and every reset, including
model, firmware and cartridge transitions. Waits are counted at fetch events,
independently of instruction length, data reads and R-register values or writes.
For indexed-CB instructions, displacement and final opcode bytes are ordinary
reads: only the index and CB prefixes incur fetch waits.

Interrupt acknowledgement is separate from ordinary fetch timing. NMI remains
11 T, IM0 with the MSX bus's FFh (RST 38h) and IM1 remain 13 T, and IM2 remains
19 T in both policies. R increments on acknowledgement without adding a fetch
wait; subsequent handler opcodes do incur waits. Standalone IM0 continues to
execute the bus-supplied opcode. The basis is Zilog UM0080 printed pages 12–14
(acknowledgement and HALT cycles) and the openMSX timing definitions linked below.
The built-in I/O wait already included in bare instruction timings is unchanged.

Original `cpu.rs` regression vectors explicitly check standalone/MSX counts:
NOP and HALT entry/idle 4/5, LD A,n 7/8, immediate IN/OUT 11/12,
register CB rotate and NEG 8/10, LD IX,nn 14/16, indexed-CB BIT 20/22 and
rotate 23/25, LDIR repeating/final 21/23 and 16/18, and DD FD NOP 12/15.
They also cover taken/untaken branches, repeated/ignored prefixes, LD R,A,
LD A,R, refresh wrap/bit 7, EI delay, HALT exit and handler fetches. An entire
64 KiB address space of index prefixes yields after 262144/327680 T to keep the
scheduler bounded. Every new CPU timing step checks returned cycles against its
`tstates` increment.

`tests/fetch_timing.rs` checks both machine generations with original RAM code,
VDP/IRQ and FDC byte-transfer boundaries, PSG sample counts/waveforms, SCC
oscillator phase, MSX2 command completion and RTC rollover. It checks lifecycle
transitions and NTSC/PAL frame overshoot accumulation. CPU `tstates`, returned
cycles, device advancement and frame budgets all use the same elapsed time.
Clocks remain 3,579,545 Hz for the CPU and 44,100 Hz for audio sampling; frame
budgets remain 228 × 262 (NTSC) and 228 × 313 (MSX2 PAL). The Wasm frame-budget
export and browser frame accumulator retain these budgets and clock rates.

The shared WD2793 [Type-I scheduler](disks.md) uses absolute fractional-cycle
deadlines for 6/12/20/30 ms steps, 30 ms verification settling and a synthetic
normal-track ID/index search. It consumes events chronologically and independently
of tick partition, preserving issued movement on interruption. Positioning runs
independently of READY and routes pulses/sensors through drive selection; only A
is installed. The first eligible pulse is issued at command start; internal startup
latency, full HLD lifecycle, spin-up and flux timing remain omitted.

This is instruction-level accounting. Devices advance after each instruction;
individual access phases, VRAM contention, V9938 arbitration and other
device-specific timing approximations remain separate work. No openMSX trace
comparison or physical hardware measurement was performed. Specification-based
tests establish this correction, not broad commercial-game playability.

## Hardware tests

`hardware.rs` checks slot visibility/write protection, PPI row and bit operations,
joystick input, ROM validation, native boot and system-ROM separation. VDP assertions
use exact palette pixels for all four modes, read-ahead and address wrapping,
VBlank timing/status clearing, sprite priority/magnification/early clock, overflow
and collision including transparent sprites. PSG tests count square-wave edges at
approximately 440 Hz, distinguish noise output, check bounded finite samples and
all sixteen envelope shapes at exact cycle boundaries. The [envelope timing and
write contract](psg-envelope.md) documents the clock derivation, provisional phase
policy, independently sampled PCM oracle and original Z80/Wasm probe. These do
not measure analog fidelity.

`basic.rs` tests program flow, nested and zero-trip loops, arithmetic precedence,
strings, errors, graphics coordinates across the entire screen, tile-font retention
on CLS, and yielding/BREAK. Browser tests exercise the actual compiled Wasm through
the UI and read rendered Canvas pixels. The audio test observes real AudioBufferSource
connections and waveform samples; manual listening is documented separately.

## Specification references

These documents informed hardware behavior; no implementation code, fonts, example
programs or ROM bytes were copied from them.

- Zilog, [Z80 CPU User Manual, UM008011-0816](https://www.zilog.com/docs/z80/um0080.pdf): instruction encodings, flags, timings, reset and interrupt behavior.
- openMSX [Z80 timing definitions](https://github.com/openMSX/openMSX/blob/master/src/cpu/Z80.hh) and [CPU implementation](https://github.com/openMSX/openMSX/blob/master/src/cpu/CPUCore.cc): upstream evidence for MSX fetch waits, indexed-CB operand reads and interrupt-entry totals; consulted for the timing contract, independently implemented here without copying source.
- Texas Instruments, [Video Display Processors Programmer's Guide (1984)](https://map.grauw.nl/resources/video/ti-vdp-programmers-guide.pdf): VDP registers, ports, modes and sprite behavior.
- General Instrument, [AY-3-8910/8912/8913 data sheet](https://www.ym2149.com/ay8910.pdf): register masks, mixer, noise, amplitude and envelope operation.
- ASCII, [MSX Technical Data Book](https://www.msxarchive.nl/pub/msx/docs/manuals/msxtech.pdf): MSX memory/I/O architecture (archive may restrict access).
- [MSX I/O ports overview](https://map.grauw.nl/resources/msx_io_ports.php) and [international keyboard matrices](https://www.msx.org/wiki/Keyboard_Matrices): supplementary MSX wiring references.
- C-BIOS Association, [project description and license](https://cbios.sourceforge.net/): optional, externally supplied BIOS test input; no BASIC, not distributed by this project.

## Cartridge mapper references and tests

Auto inference is independently implemented in `cartridge/detect.rs`, using local
AB entry points, bounded instruction tracing, competing register-window scores and
known banked code transfers. It uses no external heuristic source or ROM database.
See [the algorithm, exact thresholds, ABI and limits](mapper-detection.md).

The mapper module and generated tests are original code. Register ranges, power-on
banks and Konami mirror wiring were cross-checked against the openMSX project's
[ASCII8](https://github.com/openMSX/openMSX/blob/master/src/memory/RomAscii8kB.cc),
[ASCII16](https://github.com/openMSX/openMSX/blob/master/src/memory/RomAscii16kB.cc),
[Konami](https://github.com/openMSX/openMSX/blob/master/src/memory/RomKonami.cc) and
[Konami SCC](https://github.com/openMSX/openMSX/blob/master/src/memory/RomKonamiSCC.cc)
implementations. No implementation source or ROM bytes were copied.

`tests/cartridge.rs` checks both ends of register ranges, ignored writes, bank
masking, mirrored reads, SCC enable decoding, slot selection, reset, replacement,
ejection and failed-load preservation. Generated Z80 code switches a bank and
executes from it on every supported mapper. Browser tests generate larger images
in memory and select them through the real file input, verify pixels and Space
responses after successful and failed imports, and restore plain cartridges and
BASIC. A separate raw Wasm test verifies memory growth and invalid transfer lengths,
mapper IDs and unrepresentable allocation sizes. [ASCII cartridge SRAM and R-Type](cartridge-sram.md)
add per-cartridge battery memory, allocation-safe register reset, five explicit
mapper IDs, bounded Wasm save views/restores, and validated browser-local saves.

Classic SCC registers/audio are independently implemented from public hardware
experiments and register documentation, without emulator implementation code. See
[the sources, register map, mixer, accuracy limits and original fixture](scc.md).

ROMless Snatcher/SD Snatcher SCC-I profiles explicitly replace slot 1 with banked
64 KiB volatile RAM and compatible/plus sound registers. They retain the selected
system firmware and independently installed slot-2 disk interface. The fifth wave
is independent in plus mode. See [SCC-I decoding, reset, provenance and independent
CPU disk fixtures](scc.md#romless-snatcher-and-sd-snatcher-scc-i).
