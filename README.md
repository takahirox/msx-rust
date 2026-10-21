# msx-rust

An original Rust MSX1 / MSX2 emulator with a browser/WebAssembly frontend, Canvas video,
keyboard and joystick input, Web Audio PSG and classic SCC sound, local ROM loading,
and a usable open-source BASIC environment. The core has no third-party dependencies
or browser APIs.

**Demo target:** <https://takahirox.github.io/msx-rust/>. Publication and post-merge
verification are pending; this URL is a deployment target, not a claim of a live site.

## Run locally

Install [Rust through rustup](https://rustup.rs/), Python 3, and Node.js 22 or newer
(Node is only needed for browser tests). The checked-in toolchain configuration requests
Rust 1.99.0, rustfmt, Clippy, and `wasm32-unknown-unknown`.

```sh
rustup show
./scripts/build.sh
python3 -m http.server 41873 --bind 127.0.0.1 --directory dist
```

Open <http://127.0.0.1:41873/>. Serve over HTTP; opening `index.html` as a file does
not allow Wasm fetching. If a separate system/Homebrew Rust shadows rustup, put
the rustup toolchain first in `PATH` so Cargo, rustc, and rustdoc use the same toolchain:

```sh
export PATH="$(rustup run 1.99.0 rustc --print sysroot)/bin:$PATH"
```

The display is the main workspace. Click or tap it to focus, then use a **physical
keyboard** (touch typing / an on-screen keyboard is not supported). The focus cue
changes to **Keyboard active**; Tab moves back to the page controls. Enter:

```basic
10 FOR I=1 TO 5 : PRINT I : NEXT I
RUN
```

The screen prints 1 through 5. `LIST` displays the program. Replace a numbered line by
entering it again, or delete it by entering its number alone. Left/Right, Home, End,
Backspace and Delete edit the current line. ASCII text can also be pasted. Escape
interrupts execution. **Reset** clears the machine and program while keeping selected
ROMs. **Use Weave BASIC** clears firmware overrides and ejects cartridges on either
model. **Pause / Resume** controls emulation. The compact toolbar also has **Open
ROM**, **Eject**, and **Fullscreen**. Fullscreen keeps the toolbar accessible; **Exit
fullscreen** or Escape returns to the page. Browsers without fullscreen support get a
focused view with the same controls and exit options. BASIC programs and ROM
images are not stored. Supported ASCII cartridge SRAM is saved locally; see [cartridge saves](docs/cartridge-sram.md#browser-saving-and-recovery).

Try **BASIC example**, **Graphics demo**, or **Sound demo** beneath the display.
The starters enter commands into the current built-in BASIC session, resume paused
emulation, and restore keyboard focus. Graphics draws a sine curve using the
emulated VDP; Enter returns to text. Sound demo enables audio and plays a sustained
tone; **Stop tone** silences it. You can also **Enable audio**, then type `BEEP`
and silence it with `SOUND 8,0`. **Mute audio** turns browser playback off;
**Resume audio** restores it after browser suspension. Audio requires a user gesture.

Expand **Keyboard & BASIC help** for editing and input guidance, **ROMs & advanced
system options** for BIOS loading and compatibility/legal details, or **BASIC text
transcript** for the full built-in interpreter output. Weave BASIC is our original
interpreter, not original Microsoft MSX BASIC.

## Demo ROMs

Choose a title in **Demo ROMs** below the display, then **Play demo**. No local
ROM or account is needed. The app selects the declared machine and Plain mapper,
boots the bundled C-BIOS and focuses the display. plustaker starts immediately;
press Space/Z at YOKO’s start prompt:

| Demo | Machine | Play |
| --- | --- | --- |
| plustaker, by m@3 | MSX1 | Arrows move; Space/Z fires or restarts; shoot falling blocks and collect characters for combos. |
| YOKO (yokosht), by m@3 | MSX1 | Arrows move; Space/Z starts/fires or restarts; C uses a stocked bomb. Dodge enemies in a side-scrolling shooter. |
| Original MSX2 diagnostic | MSX2 | Hold 8 for 256-color video, 7 for 512-wide palette video; Space plays a tone after Enable audio. |

YOKO's on-screen “A” means the fire button. Prefer Space/Z and C: the existing
browser X mapping also presses joystick trigger 2, so it can fire and use a bomb
together. A physical keyboard is required. Enable audio separately to hear PSG.

Switching demos replaces the cartridge and discards unsaved BASIC. **Reset**
restarts the cartridge; **Eject** returns to BASIC in the same model under automatic
policy. **Use Weave BASIC** clears explicit firmware overrides and the cartridge.
Explicit C-BIOS remains selected across demo/model changes. Local firmware and
the diagnostic loader are retained for same-model demos; a cross-model demo is
rejected with the session unchanged until you clear that override. **Open ROM**
and the BASIC starters remain available. Failed fetches, integrity checks or
unsupported imports preserve the running session; retry Play demo.

The two unmodified third-party ROMs retain their upstream **MIT** licenses. The
[manifest](demos/manifest.json) pins official source/download revisions, sizes,
SHA-256 hashes and metadata; [redistribution and asset audit](demos/PROVENANCE.txt)
records the binary, graphics, font, music and runtime review. The selector links
individual licenses and sources. These are verified local static assets; no
third-party runtime downloads are made. Free downloads and online playability do
not by themselves grant redistribution rights. Westen House was excluded because
independent permission for credited third-party routines could not be verified.
The emulator's MIT license does not relicense others' ROMs.

The browser suite observes both games' actual MSX1 Canvas output and input-driven
sprite movement, plus MSX2 diagnostic video/input, at `/` and `/msx-rust/`. It also
checks Reset/Eject/BASIC, switching, explicit firmware, failed/tampered assets and
stale asynchronous imports. Tests step the real Wasm CPU deterministically for
gameplay assertions. They do not establish game completion, full audio accuracy,
physical hardware behavior or MSX2 backward compatibility for these MSX1 games.
Screenshots are `test-results/demo-*.png` (CI `browser-evidence` artifact).

## MSX2

Each fresh page load starts **MSX2** in original, independent **Weave BASIC**,
with the V9938 and 128 KiB VRAM profile. Choose **MSX1** in **Machine** to use
that model; reloads and new tabs start MSX2 again without remembering a preference.
Both models support Weave BASIC.
Selecting a machine clears incompatible ROMs and starts fresh BASIC. Under the
default automatic policy, **Open ROM** switches to the matching bundled
**C-BIOS 0.29a** (MSX1 main or MSX2 main + sub-ROM) before Z80 cartridge boot.
**Eject** returns to fresh BASIC in the same model. **Reset** retains firmware,
cartridge and mapper, restoring initial banks. Opening a cartridge or changing
firmware resets execution and **discards the in-memory BASIC program**.

Under **ROMs & advanced system options → Firmware policy**, explicitly selecting
C-BIOS, the MSX2 **Original diagnostic loader**, or local firmware sets an override
that survives Open ROM, Reset and Eject. C-BIOS is a separate cartridge BIOS with
**no MSX BASIC**. The diagnostic loader has no standard BIOS API. Selecting either
bundled override ejects cartridges and clears local firmware selections.
**Use Weave BASIC** clears the override and cartridge, restoring automatic behavior.
No Microsoft MSX BASIC 2.0 or commercial cartridge compatibility is claimed.

Download our original **C-BIOS service-test cartridge** under advanced options and
open it to exercise standard BIOS calls, expanded slots, video, keyboard and PSG.
Space changes a black tile to white and plays a tone after enabling audio. The
separate original MSX2 diagnostic demonstrates V9938 512×212 palette video,
256-color video (hold 8; hold 7 to restore), extended VRAM, commands, sprites and RTC.

For external firmware, select a matching local **32 KiB MSX2 main BIOS/BASIC** and
**16 KiB sub-ROM**, then **Apply MSX2 firmware**. The pair is validated before
replacing active firmware and restarting the current cartridge. Files remain local
and are never uploaded or saved; failed imports preserve the current session.
See the [MSX2 profile, provenance, limitations and verification](docs/msx2.md).

## Hardware and compatibility

| Component | Initial implementation |
| --- | --- |
| Z80 | Main/alternate registers, flags, base/CB/ED/DD/FD/indexed-CB instruction families, stack, I/O, HALT, EI delay, IRQ/NMI, IM 0/1/2 and instruction T-states |
| Memory (MSX1) | Four primary slots, four independently selected 16 KiB pages, 64 KiB RAM in slot 3, 32 KiB system ROM in slot 0, plain/MegaROM cartridge in slot 1 |
| MSX2 | V9938, 128 KiB VRAM, 64 KiB RAM, expanded slot 3 with sub-ROM in 3-0 and RAM in 3-3, deterministic RTC; [coverage](docs/msx2.md) |
| TMS9918A (MSX1) | 16 KiB VRAM, two-write control latch, read-ahead, text/Graphics I/Graphics II/multicolor, 8/16 pixel sprites and magnification, priority, collision/fifth-sprite flags, VBlank IRQ, 256×192 RGBA output |
| Konami K051649 SCC | Five wavetable voices, four signed 32-byte waves (channels 4/5 share), 12-bit periods, linear volumes, classic test modes; mixed with PSG at 44.1 kHz |
| AY-3-8910 | Three tone channels, noise LFSR, mixer, amplitude/envelope registers, logarithmic levels, 44.1 kHz mono samples |
| Input/I/O | PPI primary-slot register, keyboard row selection and bit set/reset, international keyboard matrix, PSG joystick 1 input; arrows + Z/X are directions/triggers |
| Browser | Frame accumulator at approximately 59.92 Hz, bounded catch-up, key release on focus loss, Canvas, user-activated Web Audio, local file pickers |

This is an initial implementation, not a claim of universal MSX software compatibility.
Instruction-level timing is used; individual bus phases, MSX M1 wait states, analog
video/overscan, VDP access contention, and undocumented CPU silicon quirks are not
accurately modeled. Sprite status at raster/off-screen edges and unusual combined
VDP modes are not compatibility guarantees. There are no disk/tape drives,
SCC+/SCC-I or FM audio expansions. MSX2 coverage and optional peripheral limits
are documented separately. ASCII8, ASCII16, Konami Standard and Konami SCC banking
with classic SCC audio, ASCII8/ASCII16 2/8 KiB SRAM variants and the 384 KiB
R-Type layout are supported. Other specialized mappers and Game Master 2
(including its second-slot use) remain unsupported.

## Weave BASIC and firmware

**Weave BASIC is a complete small interpreter for the supported language, not
Microsoft MSX BASIC or a compatible MSX BIOS implementation.** Its original source
is in `crates/msx-core/src/basic.rs`. The bundled Z80 boot ROM polls keyboard events
and invokes this Rust firmware service through private ports F0/F1. The interpreter
writes the real emulated VDP tables and PSG registers; the browser contains no BASIC
parser and does not draw a separate text or graphics mockup. `firmware/boot.asm`
and `crates/msx-core/src/firmware.rs` contain the boot source and exact byte generator.
The service ports are disabled with C-BIOS or a user system ROM.

Supported commands: `PRINT` (strings/numbers, `;` and 14-column comma zones),
`LET` or bare assignment, `IF ... THEN` statement/line number, `GOTO`,
`GOSUB` / `RETURN`, nested `FOR ... TO ... STEP ...` / `NEXT`, `DIM`,
`DATA` / `READ` / `RESTORE`, interactive `INPUT`, `LIST`, `RUN`, `NEW`, `END`,
`STOP`, `REM`/apostrophe comments, `HELP`, `CLS`, `SCREEN`, `COLOR`, `PSET`,
`LINE`, `CIRCLE`, `PAINT`, `VDP`, `VPOKE`, `SOUND` and `BEEP`. Expressions support
parentheses, arithmetic, powers, comparisons, boolean `AND`/`OR`/`NOT`, `ABS`,
`INT`, `SQR`, `SIN` and `COS`. Variables are case-insensitive, default to zero
(empty for `$` strings), and use f64 numbers. Names retain all their characters;
numeric output uses compact decimal formatting rather than MSX's surrounding spaces.

- `GOSUB 100` returns to the following statement, including after a colon. Calls
  nest up to 64 deep; `RETURN` discards unfinished loops inside that call while
  preserving caller loops. Direct `GOSUB` and `GOTO` can reach stored lines.
  Bare `RETURN` without a call is an error; `RETURN line` is unsupported.
- `DIM A(10),N$(2,3)` explicitly allocates one- or two-dimensional arrays, separate
  from scalars. Bounds are inclusive, zero-based integers from 0 to 4095, with
  16,384 total elements across arrays. Unlike MSX BASIC, implicit dimensioning is
  unsupported. Wrong dimensions, fractional/negative indices, out-of-bounds
  accesses, repeated `DIM`, and type mismatches are errors.
- `DATA -2,HELLO,"A:B,C"` stores literal fields in numbered-line order, including
  multiple `DATA` statements on a line. Quote commas/colons; unquoted strings may
  contain spaces. `READ A,N$` advances through those fields; numeric fields must
  parse as finite decimal numbers (scientific notation is accepted in DATA/INPUT).
  Type errors leave the offending field unread; exhaustion reports `OUT OF DATA`.
  `RESTORE` rewinds; `RESTORE 100` starts at the first DATA at/after the existing
  line 100. Direct `DATA` is a no-op and is not added to the stored data.
- `INPUT "NAME";N$` or `INPUT A,N$` waits in SCREEN 0 using the same ASCII editor
  and keyboard/paste path as commands. Enter one comma-separated field per target;
  quote strings containing commas. Wrong field counts, malformed quotes or invalid
  numbers print `?REDO FROM START` and retry the whole input without assignments.
  Array targets are resolved when INPUT starts. The prompt remains visible (long
  prompts are shortened in the editor). Escape cancels the program and partial
  input; Reset clears the machine. No polling loop blocks the browser while waiting.
- `RUN` clears scalars/arrays and rewinds DATA. `NEW` also removes the program.
  Editing a stored line rewinds DATA. Direct commands otherwise retain variables
  and the data position. End-of-program, `END`, `STOP`, errors and Escape clear
  pending calls, loops and INPUT. `RUN` in a program restarts with fresh state.
  `STOP` prints BREAK with its program line; resumable `CONT` is unsupported.
  Execution yields each frame, including infinite `GOTO`/`RUN` programs.

FOR loops with an initially exhausted range execute zero times (a deliberate deviation
from MSX BASIC's initial iteration); NEXT accepts one variable at a time. Input is
ASCII, up to 240 characters per editor line, with horizontal scrolling; lowercase
glyphs display as uppercase. There are at most 64 nested loops. Use `PRINT` and
`INPUT` in text mode. `SCREEN 0` is 40-column text, `SCREEN 1` is tile graphics,
`SCREEN 2` is 256×192 graphics with the TMS9918A
one-foreground/background-pair-per-eight-pixels restriction, and `SCREEN 3` uses 4×4
color blocks. Graphics statements use emulated VRAM in modes 2/3 and MSX2 bitmap modes
5–8:

```basic
PSET (128,96),10
LINE (16,16)-(239,175),15,B
LINE (32,32)-(63,55),6,BF
CIRCLE (128,96),40,15
PAINT (128,96),10,15
```

`PSET x,y,color` is also retained. Color is optional (defaults to the foreground
set by `COLOR fg,bg`), with values 0–15. `LINE` supports plain segments, `B` boxes
and `BF` filled boxes, omitted start (`LINE -(x,y)`), and `STEP(x,y)` relative
coordinates. Coordinates round to integers and accept −32768 through 32767, including
the result of relative offsets; off-screen drawing is clipped. `CIRCLE` supports full circles
with integer radii 0–1024; arcs and aspect parameters are unsupported. `PAINT`
supports optional fill and border colors (border defaults to fill), filling the
four-connected region bounded by the border color or screen edges. It snapshots
VRAM boundaries and visits at most 49,152 pixels (3,072 blocks in mode 3), so an
open or same-color fill terminates. An explicit border is supported in both modes,
including SCREEN 2 as a Weave extension. These bounded primitives run within one
statement and yield before the next statement to keep repeated drawing responsive.
Mode 2 drawing can recolor other foreground pixels in the same eight-pixel group;
drawing the group's background color clears the corresponding pattern bit.
Color 0 displays as the VDP backdrop. `SCREEN` initializes colors and drawing
position; `CLS` clears graphics without resetting the position.

On MSX2, Weave BASIC additionally supports non-interlaced, page-zero bitmap
`SCREEN 5` (256×212, palette 0–15), `SCREEN 6` (512×212, palette 0–3),
`SCREEN 7` (512×212, palette 0–15) and `SCREEN 8` (256×212, direct GRB 0–255).
`COLOR fg,bg` sets the default drawing color and backdrop; bitmap `CLS` clears to
opaque color zero. SCREEN 8 packs green in bits 7–5, red in 4–2 and blue in 1–0
(V9938 DAC levels). SCREEN 5/6/7 use the V9938 power-on palette. All existing
PSET/LINE/CIRCLE/PAINT forms apply, with clipping to the dimensions above and
bounded fill work (at most 108,544 pixels). `SCREEN` restores supported display
settings; Enter or Escape returns to 40-column text. SCREEN 4, 80-column PRINT,
bitmap text/INPUT, interlace, page selection, sprite commands and disk BASIC
are outside this language subset. The V9938 hardware can support additional modes
when programmed by cartridges. This is not full Microsoft MSX BASIC 2.0.

Low-level `VDP register,value`, `VPOKE address,value`, and `SOUND register,value`
use decimal parameters. `BEEP` sustains a tone until silenced.
Still unsupported: `ELSE`, automatic arrays, integer/single/double type suffixes,
`PLAY`, sprite commands, files/disk/tape, binary tokenized programs, MSX BIOS entry
points and MSX2 BASIC in C-BIOS mode. This is an independently implemented,
MSX BASIC-inspired language subset, not Microsoft's original BASIC.

### Original BASIC examples

These examples, fixtures and interpreter additions are independently authored under
this repository's MIT license. Copy/type each listing into a fresh Weave BASIC session
on either machine (Reset or `NEW`), then enter `RUN`. They are also in
[examples/basic](examples/basic) and the browser's **Keyboard & BASIC help**.

Array/data processing with a subroutine:

```basic
10 DIM A(3),N$(3)
20 FOR I=1 TO 3:READ N$(I),A(I):NEXT I
30 FOR I=1 TO 3:GOSUB 100:NEXT I
40 END
100 PRINT N$(I);":";A(I):RETURN
200 DATA COMET,3,ORBIT,1,MOON,2
RUN
```

Interactive greeting (enter a name, then a count from 1 to 5; Escape cancels):

```basic
10 INPUT "NAME";N$
20 INPUT "COUNT (1-5)";N
30 IF N<1 OR N>5 THEN 20
40 FOR I=1 TO N:PRINT "HELLO ";N$:NEXT I
50 END
RUN
```

Graphics (Enter returns to text after the program finishes):

```basic
10 SCREEN 2
20 LINE (16,16)-(239,175),15,B
30 CIRCLE (128,96),40,15
40 PAINT (128,96),10,15
50 LINE (32,32)-(63,55),6,BF
60 END
RUN
```

### Language references and provenance

Syntax and observable behavior are informed by the publicly available
[MSX Technical Data Book, MSX BASIC Reference Guide, chapter 2](https://map.grauw.nl/resources/system/msxtech.pdf)
(printed pp. 65, 68–71, 73, 77–78 and 84–88: variable/array, flow, DATA/INPUT
and graphics statement descriptions), alongside the
[TMS9918A hardware references](docs/architecture.md). The bounds, reset behavior,
INPUT retry policy and unsupported forms above are deliberate limits of Weave BASIC.
Only language/hardware documentation was used: no proprietary ROM, implementation,
disassembly, token table, or third-party interpreter code was used or bundled.

## ROM loading and legal guidance

No proprietary BIOS, BASIC or commercial game ROM is included. Curated third-party
games retain their upstream licenses; see [Demo ROMs](#demo-roms).
The emulator, original glyphs,
cartridges and seven generated ROMs are MIT licensed; the bundled C-BIOS MSX1 main and
MSX2 main/sub-ROMs are separately **BSD-2-Clause**, with its complete upstream
copyrights, conditions and disclaimer in
[CBIOS-LICENSE.txt](firmware/cbios/CBIOS-LICENSE.txt). [The pinned release
record](firmware/cbios/CBIOS-PROVENANCE.txt) includes the official archive URL, exact
version and archive/ROM SHA-256 hashes. See [LICENSE](LICENSE) and the [asset
provenance inventory](firmware/ROM-LICENSE.txt). `./scripts/build.sh` generates the
original ROMs and copies the three unmodified C-BIOS ROMs and notices into `dist/`.
The core embeds the same binaries in Wasm, so normal boot needs no separate firmware
download or SourceForge connection. Rust has no external crate dependencies;
Playwright is a development-only dependency under its upstream license and is not
shipped in the site.

Expand **ROMs & advanced system options** to download the original **test cartridge**,
then choose it with **Open ROM** in the toolbar. It displays a title and sprite;
hold Space to play a roughly 440 Hz PSG tone.
This cartridge runs Z80 code against standard hardware ports, with no native BASIC
service calls. Select `.rom` or `.bin` cartridge files locally. There is **no fixed
upload size limit**: available browser/Wasm memory and the selected mapper's hardware
address range apply. No image is truncated to fit.

Choose **Cartridge mapper** under **ROMs & advanced system options** before selecting the file with **Open ROM**:

- **Auto** (default) recognizes plain 8, 16, 32 and 48 KiB AB-header layouts and
  estimates ASCII8, ASCII16, Konami or Konami SCC from reachable Z80 mapper writes
  and banked code structure. A MegaROM needs sufficient evidence and a clear lead
  over competing candidates; size and the AB header alone cannot choose its mapper.
  Successful MegaROM imports display **Auto: mapper, estimate**. Uncertain imports
  report candidate scores and leave the running cartridge and machine intact.
  Choose a documented mapper manually and select the same local file again to retry.
  See the [independent method, thresholds and limits](docs/mapper-detection.md).
- **Plain** uses the same header validation. Small images mirror in 4000h–BFFFh;
  48 KiB images start at 0000h. These lengths describe a mapping format, not an
  upload cap.
- **ASCII8**, **ASCII16**, **Konami Standard**, **Konami SCC**,
  **ASCII8SRAM2/8**, **ASCII16SRAM2/8**, and **R-Type** explicitly select
  MegaROM hardware (Konami SCC includes classic wavetable audio), bypassing
  heuristic choice while keeping format validation.
  Use the ROM author's documentation to choose; arbitrary or ambiguous images
  cannot be identified reliably. Change the mapper and select
  the same file again to retry. Changing the picker alone leaves the running
  cartridge unchanged.

| Mapper | CPU bank windows | Bank-register write ranges (inclusive) | Reset banks | Hardware address range |
| --- | --- | --- | --- | --- |
| ASCII8 | Four 8 KiB banks at 4000h–BFFFh | 6000h–67FFh, 6800h–6FFFh, 7000h–77FFh, 7800h–7FFFh | 0, 0, 0, 0 | 2 MiB (8 bank bits) |
| ASCII16 | Two 16 KiB banks at 4000h–BFFFh | 6000h–67FFh, 7000h–77FFh | 0, 0 | 4 MiB (8 bank bits) |
| ASCII8SRAM2/8 | Four 8 KiB banks, 2/8 KiB SRAM | ASCII8 ranges above | 0, 0, 0, 0 | 1 MiB ROM; SRAM select = ROM length / 8192 |
| ASCII16SRAM2/8 | Two 16 KiB banks, 2/8 KiB SRAM | ASCII16 ranges above | 0, 0 | 256 KiB ROM; SRAM select = bit 4 |
| R-Type | Fixed 16 KiB bank 0Fh, switched 16 KiB window at 8000h | 7000h–7FFFh | 0Fh, 0 | Exactly 384 KiB; bit 4 selects 256/128 KiB chips |
| Konami | Four 8 KiB banks; first fixed at 0 | 6000h–7FFFh, 8000h–9FFFh, A000h–BFFFh | 0, 1, 2, 3 | 256 KiB (5 bank bits) |
| Konami SCC | Four 8 KiB banks at 4000h–BFFFh | 5000h–57FFh, 7000h–77FFh, 9000h–97FFh, B000h–B7FFh | 0, 1, 2, 3 | 512 KiB (6 bank bits) |

Except for the explicit 384 KiB R-Type layout, MegaROM images must have a
power-of-two byte length of at least one mapper bank, with an AB header visible in the initial banks. Unconnected bank address bits
mirror smaller ROMs. Images beyond the hardware address range need a different
or extended mapper; they are rejected with an explanation. ASCII mappings are
unmapped outside 4000h–BFFFh. Konami mirrors page 1 into page 0 and page 2 into
page 3; SCC swaps those mirrors. Only writes through the selected cartridge slot
change banks. **Classic SCC audio is supported**: selecting bank 3Fh in the third
SCC window exposes waveform RAM and sound registers at 9800h–9FFFh. Five independent
voices share four waveforms (channels 4/5 share storage), and mix with PSG. Konami
Standard has different banking and no SCC. See [the SCC register map, timing,
accuracy limits and tests](docs/scc.md).
SRAM is readable through selected windows and writable only at 8000h–BFFFh in
SRAM mode. The 2/8 KiB memory repeats within those windows. Reset retains it.
Auto cannot establish SRAM hardware or its size; choose these five new mappers
manually. See [exact wiring, limits, original sources and fixtures](docs/cartridge-sram.md).
SCC+/SCC-I, reversed ASCII registers, 32 KiB SRAM, other select wiring and
Game Master 2 / additional cartridge slots remain unsupported.

SRAM saves are local to this browser, keyed by the ROM bytes' SHA-256 and selected
mapper. Reopen the same ROM and mapper after a fresh page to restore the save.
Advanced options show storage errors and provide **Export SRAM backup** and
**Import and replace SRAM**. Backups validate version, ROM identity, mapper, size
and checksum before changing SRAM. Export for safekeeping: private mode or cleared
browser data can remove local saves. Corrupt/conflicting records are preserved
until explicit import; storage failures allow continued play and export. Original
ROM images are never persisted or sent to a server. [Saving and recovery details](docs/cartridge-sram.md#browser-saving-and-recovery).

Loading a valid image resets the machine and banks. Reset retains selected ROMs and
restores initial banks without erasing cartridge SRAM; Eject removes the cartridge
and returns automatic mode to BASIC, retaining explicit firmware overrides; Use Weave BASIC clears the override and
cartridge. The browser saves outgoing SRAM before cartridge, model or firmware
changes. Failed imports leave the running machine, banks and unsaved SRAM intact.
The Wasm transfer reserves memory before JavaScript creates a writable view, then
moves that buffer into the core without another cartridge copy. Empty files, malformed
dumps, unsupported mapping and resource failures are reported as nonfatal errors.
Extremely large files remain subject to browser ArrayBuffer limits and the 32-bit Wasm
address space.

Inference uses the same local bytes under MSX1 and MSX2 and never changes the
machine model. A likely mapper does not identify a game, prove it boots, select
a compatible user BIOS/sub-ROM, or add unsupported hardware.

The original diagnostic loader initializes self-contained cartridges using their AB
init pointer; many commercial cartridges need standard BIOS services. Both models
automatically select their matching C-BIOS for that purpose, with limited
compatibility. No commercial-game compatibility or manual copyrighted-game play-testing is required or claimed.

Download the original **SCC test cartridge** in advanced options and open it with
Konami SCC or Auto on either model. After enabling audio and focusing the display,
hold Space for a 440 Hz SCC square, Up for all five SCC voices, Left for PSG,
or combine Left with Space/Up. Right disables the SCC overlay. Release keys for
silence; Reset restarts it and Eject returns to BASIC. Its code and synthetic
waveforms are original MIT-licensed data; no music or game assets are included.

Mapper fixtures are generated, original MIT-licensed code in
[`cartridge.rs`](crates/msx-core/tests/cartridge.rs),
[`detection.rs`](crates/msx-core/tests/detection.rs),
[`scc_fixture.rs`](crates/msx-core/src/firmware/scc_fixture.rs),
[`sram_fixture.rs`](crates/msx-core/src/firmware/sram_fixture.rs) and
[`browser.spec.js`](tests/browser.spec.js). Run the verification commands below to
test legally distributable images: browser fixtures of 64, 128, 256, 512 and
1024 KiB use Auto, switch to code in the last bank, display a backdrop and respond
to Space through the real file picker and Wasm. Rust tests also cover 2/4 MiB
images, register boundaries, reset, slot effects and invalid-load preservation.
No fixture downloads or proprietary ROMs are required. These synthetic tests verify
specified evidence handling, not universal detection accuracy on commercial ROMs.

Under **ROMs & advanced system options → Local system BIOS / BASIC**, select a
lawfully obtained 32 KiB MSX1 BIOS/BASIC ROM (generation byte 002Dh = 0). **Use Weave
BASIC** in that section unloads selected ROMs and returns to Weave BASIC in the
current model. With a system ROM selected, the CPU executes it from reset, allowing
original BIOS/BASIC behavior where compatible. Files are read using
`File.arrayBuffer`, never uploaded or persisted. Do not commit or redistribute user
ROMs unless you independently establish permission. Original proprietary
Microsoft/ASCII BIOS/BASIC has not been validated here.

[C-BIOS](https://cbios.sourceforge.net/) 0.29a is bundled for MSX1 and MSX2, with **no original
MSX BASIC**. No C-BIOS logo, BASIC, disk or music ROM is included. The historical
[validation record](docs/validation.md) describes earlier external-firmware tests;
[current bundled-path checks](docs/msx2.md#verification) require no user firmware.

## Development and verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
./scripts/build.sh
node scripts/check-dist.mjs
npm ci
npx playwright install chromium
npm test
```

Stop the manual server first: browser tests start their own servers on 41873/41874 and
intentionally refuse to reuse an unrelated service. On Linux CI, use
`npx playwright install --with-deps chromium` to install browser system libraries.
The browser suite types through keyboard events, verifies BASIC digits directly in
Canvas pixels, tests editing and BREAK, checks graphics, observes nonzero PSG
waveforms and fresh muted buffers below 0.0001 after bounded settling in a running
AudioContext, and uses real file inputs for ROMs.
The suite also checks starter actions, scoped keyboard focus, loading/failure
feedback, native fullscreen and its unavailable/rejected API fallbacks, expandable
help/BIOS/transcript controls, and desktop/tablet/mobile layouts (including 320px
width). Screenshots of BASIC, graphics, cartridge, responsive layouts, expanded
details and fullscreen go to `test-results/`; CI uploads them as `browser-evidence`.
Automated audio checks verify the browser signal path, not physical speakers.

The browser tests run automatic BASIC/cartridge transitions for both models at both
site roots, verify real standard-BIOS cartridge pixels, keyboard input and
audio/silence, and check firmware switching, local pairs, privacy and restoration of
MSX1. They also check 512/256-pixel modes, 212 lines, all 256 direct colors and
sprite/command pixels. Screenshots go to `test-results/msx2-screen7.png` and
`msx2-screen8.png`.

The Rust tests include exhaustive arithmetic/BCD cases, instruction-family vectors,
interrupt and timing checks, VDP port/mode/sprite/IRQ tests, PSG waveform/envelopes,
slots/PPI, plain and bank-switched cartridge execution, BASIC syntax/runtime tests,
classic SCC register/timing/mixing/lifecycle tests, and supplied-ROM boot tests.
See [test methodology and hardware references](docs/architecture.md)
and the [manual smoke flow and actual results](docs/validation.md).
See the [UI redesign validation and screenshots](docs/ui-validation.md) for Issue #7.

## Deployment

[The Pages workflow](.github/workflows/pages.yml) builds the static site with the
pinned Rust compiler, runs Rust checks, validates its exact asset inventory, runs browser tests
including real static hosting at `/msx-rust/`, and uploads only `dist/` as the
Pages artifact. Pushes to `main` deploy only after verification; pull requests
only verify. All URLs are relative so project-subpath hosting works.
See the [deployment guide](docs/deployment.md) for reproducible commands, owner
setup, failure inspection and pending production checks.

One-time repository prerequisite: set **Settings → Pages → Build and deployment →
Source → GitHub Actions**, and allow deployment to the `github-pages` environment.
See [GitHub's custom workflow instructions](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages).
After merge, confirm the actual published URL boots, accepts keyboard input and
ROM selection, and unlocks audible sound. Publication and these checks remain pending.

## Contributing

- [Development flow and English collaboration convention](docs/development-flow.md)
- [Review and merge guidelines](docs/review-guidelines.md)
- [AI agent entry point](AGENTS.md)
