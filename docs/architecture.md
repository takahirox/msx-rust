# Implementation and validation methodology

## Boundaries

`msx-core` owns the CPU, address bus, VDP, PSG, firmware, line editor and BASIC.
It is ordinary Rust using the standard library with zero crate dependencies.
`msx-wasm` exports a small C-style ABI for frames, pixels, samples, keyboard events,
ROM staging and errors. JavaScript owns browser APIs only. All ROM generators are
ordinary Rust functions, with no assembler, downloaded artifact, or external ROM
needed to build or boot.

`Machine::step` executes a Z80 instruction and advances VDP and PSG time by its
T-states. `Machine::frame` preserves instruction overshoot between calls and runs
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

## CPU tests and vectors

All implementation and tests were independently written for this repository.
No existing CPU emulator source was used. The decoder follows the bit fields of the
Zilog opcode tables. The tests in `crates/msx-core/tests/cpu.rs` are executable
vectors, specifying exact instruction bytes, initial state, final state and T-states.
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
behavior, silicon-specific flags, contention, and M1 wait states. No third-party
ZEX/Fuse corpus or physical-machine differential test has been run. The supplied-ROM
integration example also boots an unmodified local C-BIOS and then checks our
independent cartridge's VDP, keyboard and PSG behavior.

## Hardware tests

`hardware.rs` checks slot visibility/write protection, PPI row and bit operations,
joystick input, ROM validation, native boot and system-ROM separation. VDP assertions
use exact palette pixels for all four modes, read-ahead and address wrapping,
VBlank timing/status clearing, sprite priority/magnification/early clock, overflow
and collision including transparent sprites. PSG tests count square-wave edges at
approximately 440 Hz, distinguish noise output, check bounded finite samples and
all 16 envelope shape terminal/repeat states. These do not measure analog fidelity.

`basic.rs` tests program flow, nested and zero-trip loops, arithmetic precedence,
strings, errors, graphics coordinates across the entire screen, tile-font retention
on CLS, and yielding/BREAK. Browser tests exercise the actual compiled Wasm through
the UI and read rendered Canvas pixels. The audio test observes real AudioBufferSource
connections and waveform samples; manual listening is documented separately.

## Specification references

These documents informed hardware behavior; no implementation code, fonts, example
programs or ROM bytes were copied from them.

- Zilog, [Z80 CPU User Manual, UM008011-0816](https://www.zilog.com/docs/z80/um0080.pdf): instruction encodings, flags, timings, reset and interrupt behavior.
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
mapper IDs and unrepresentable allocation sizes. SCC registers/audio and SRAM are
explicitly outside this implementation.
