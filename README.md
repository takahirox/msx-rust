# msx-rust

An original Rust MSX1 / MSX2 emulator with a browser/WebAssembly frontend, Canvas video,
keyboard and joystick input, Web Audio PSG sound, local ROM loading, and a usable
open-source BASIC environment. The core has no third-party dependencies or browser APIs.

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

The screen prints 1 through 5. `LIST` displays the program. Replace a numbered line
by entering it again, or delete it by entering its number alone. Left/Right, Home,
End, Backspace and Delete edit the current line. ASCII text can also be pasted.
Escape interrupts execution. **Reset** clears the machine and program while keeping
selected ROMs. **Built-in BASIC** also unloads both ROM inputs. **Pause / Resume**
controls emulation. The compact toolbar also has **Open ROM**, **Eject**, and
**Fullscreen**. Fullscreen keeps the toolbar accessible; **Exit fullscreen** or
Escape returns to the page. Browsers without fullscreen support get a focused
view with the same controls and exit options. There is no persistent program or
ROM storage.

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

## MSX2

Choose **Machine → MSX2**, then **Open ROM** to boot a supported cartridge with
built-in **C-BIOS 0.29a**. This open-source, cartridge-oriented replacement BIOS
executes on the emulated Z80. It provides **no original MSX BASIC**, and game
compatibility is limited. Accepting a file or estimating its mapper does not prove
that it boots. MSX1 retains Weave BASIC.

Under **ROMs & advanced system options → MSX2 firmware**, select **Original
diagnostic loader** to restore the previous minimal loader (no standard BIOS API),
or C-BIOS to restore the default. Both choices eject the cartridge and clear local
firmware selections. **Restore built-in C-BIOS** does the same. **Reset** keeps the
active firmware/cartridge and restores mapper banks; **Eject** keeps the firmware.
Changing machine models clears selected ROMs and resets to that model's default.

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
| AY-3-8910 | Three tone channels, noise LFSR, mixer, amplitude/envelope registers, logarithmic levels, 44.1 kHz mono samples |
| Input/I/O | PPI primary-slot register, keyboard row selection and bit set/reset, international keyboard matrix, PSG joystick 1 input; arrows + Z/X are directions/triggers |
| Browser | Frame accumulator at approximately 59.92 Hz, bounded catch-up, key release on focus loss, Canvas, user-activated Web Audio, local file pickers |

This is an initial implementation, not a claim of universal MSX software compatibility.
Instruction-level timing is used; individual bus phases, MSX M1 wait states, analog
video/overscan, VDP access contention, and undocumented CPU silicon quirks are not
accurately modeled. Sprite status at raster/off-screen edges and unusual combined
VDP modes are not compatibility guarantees. There are no disk/tape drives or SCC
audio. MSX2 coverage and optional peripheral limits are documented separately.
ROM-only ASCII8, ASCII16, Konami and Konami SCC banking is supported;
SRAM and other mapper variants are not.

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

FOR loops with an initially exhausted range execute zero times (a deliberate
deviation from MSX BASIC's initial iteration); NEXT accepts one variable at a time.
Input is ASCII, up to 240 characters per editor line, with horizontal scrolling;
lowercase glyphs display as uppercase. There are at most 64 nested loops.
Use `PRINT` and `INPUT` in text mode. `SCREEN 0` is 40-column text, `SCREEN 1` is
tile graphics, `SCREEN 2` is 256×192 graphics with the TMS9918A
one-foreground/background-pair-per-eight-pixels restriction, and `SCREEN 3` uses
4×4 color blocks. Graphics statements use the emulated VRAM in modes 2/3:

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

Low-level `VDP register,value`, `VPOKE address,value`, and `SOUND register,value`
use decimal parameters. `BEEP` sustains a tone until silenced.
Still unsupported: `ELSE`, automatic arrays, integer/single/double type suffixes,
`PLAY`, sprite commands, files/disk/tape, binary tokenized programs, MSX BIOS entry
points and MSX2 BASIC in C-BIOS mode. This is an independently implemented,
MSX BASIC-inspired language subset, not Microsoft's original BASIC.

### Original BASIC examples

These examples, fixtures and interpreter additions are independently authored under
this repository's MIT license. Copy/type each listing into a fresh built-in MSX1
session (Reset or `NEW`), then enter `RUN`. They are also in
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

No proprietary BIOS, BASIC or game ROM is included. The emulator, original glyphs,
cartridges and six generated ROMs are MIT licensed; the bundled C-BIOS main/sub
pair is separately **BSD-2-Clause**, with its complete upstream copyrights,
conditions and disclaimer in [CBIOS-LICENSE.txt](firmware/cbios/CBIOS-LICENSE.txt).
[The pinned release record](firmware/cbios/CBIOS-PROVENANCE.txt) includes the official
archive URL, exact version and archive/ROM SHA-256 hashes. See [LICENSE](LICENSE)
and the [asset provenance inventory](firmware/ROM-LICENSE.txt).
`./scripts/build.sh` generates the original ROMs and copies the two unmodified
C-BIOS ROMs and notices into `dist/`. The core embeds the same pair in Wasm, so
normal boot needs no separate firmware download or SourceForge connection.
Rust has no external crate dependencies; Playwright is a development-only dependency
under its upstream license and is not shipped in the site.

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
- **ASCII8**, **ASCII16**, **Konami**, and **Konami SCC** explicitly select ROM-only
  MegaROM hardware, bypassing heuristic choice while keeping format validation.
  Use the ROM author's documentation to choose; arbitrary or ambiguous images
  cannot be identified reliably. Change the mapper and select
  the same file again to retry. Changing the picker alone leaves the running
  cartridge unchanged.

| Mapper | CPU bank windows | Bank-register write ranges (inclusive) | Reset banks | Hardware address range |
| --- | --- | --- | --- | --- |
| ASCII8 | Four 8 KiB banks at 4000h–BFFFh | 6000h–67FFh, 6800h–6FFFh, 7000h–77FFh, 7800h–7FFFh | 0, 0, 0, 0 | 2 MiB (8 bank bits) |
| ASCII16 | Two 16 KiB banks at 4000h–BFFFh | 6000h–67FFh, 7000h–77FFh | 0, 0 | 4 MiB (8 bank bits) |
| Konami | Four 8 KiB banks; first fixed at 0 | 6000h–7FFFh, 8000h–9FFFh, A000h–BFFFh | 0, 1, 2, 3 | 256 KiB (5 bank bits) |
| Konami SCC | Four 8 KiB banks at 4000h–BFFFh | 5000h–57FFh, 7000h–77FFh, 9000h–97FFh, B000h–B7FFh | 0, 1, 2, 3 | 512 KiB (6 bank bits) |

MegaROM images must have a power-of-two byte length of at least one mapper bank,
with an AB header visible in the initial banks. Unconnected bank address bits
mirror smaller ROMs. Images beyond the hardware address range need a different
or extended mapper; they are rejected with an explanation. ASCII mappings are
unmapped outside 4000h–BFFFh. Konami mirrors page 1 into page 0 and page 2 into
page 3; SCC swaps those mirrors. Only writes through the selected cartridge slot
change banks. **SCC sound/register hardware is not implemented**: selecting bank
3Fh in the third SCC window hides ROM at 9800h–9FFFh, which reads FFh and ignores
SCC writes. This supports SCC mapper address decoding, not full SCC-game compatibility.
SRAM/save hardware, SCC+, reversed ASCII registers and other mapper variants are
unsupported; manual selection cannot establish that an unknown image uses none
of these features.

Loading a valid image resets the machine and banks. Reset retains selected ROMs
and restores initial banks; Eject removes the cartridge; Built-in BASIC removes
cartridge and system firmware. Failed imports leave the running machine and its
bank selections intact. The Wasm transfer reserves memory before JavaScript creates
a writable view, then moves that buffer into the core without another cartridge
copy. Empty files, malformed dumps, unsupported mapping and resource failures are
reported as nonfatal errors. Extremely large files remain subject to browser
ArrayBuffer limits and the 32-bit Wasm address space.

Inference uses the same local bytes under MSX1 and MSX2 and never changes the
machine model. A likely mapper does not identify a game, prove it boots, select
its required BIOS/sub-ROM, or add SCC audio or other unsupported hardware.

The original diagnostic loader initializes self-contained cartridges using their AB
init pointer; many commercial cartridges need standard BIOS services. MSX2 now
defaults to C-BIOS for that purpose, with limited compatibility. No commercial-game
compatibility or manual copyrighted-game play-testing is required or claimed.

All mapper fixtures are generated, original MIT-licensed code in
[`cartridge.rs`](crates/msx-core/tests/cartridge.rs),
[`detection.rs`](crates/msx-core/tests/detection.rs) and
[`browser.spec.js`](tests/browser.spec.js). Run the verification commands below to
test legally distributable images: browser fixtures of 64, 128, 256, 512 and
1024 KiB use Auto, switch to code in the last bank, display a backdrop and respond
to Space through the real file picker and Wasm. Rust tests also cover 2/4 MiB
images, register boundaries, reset, slot effects and invalid-load preservation.
No fixture downloads or proprietary ROMs are required. These synthetic tests verify
specified evidence handling, not universal detection accuracy on commercial ROMs.

Under **ROMs & advanced system options → Local system BIOS / BASIC**, select a
lawfully obtained 32 KiB MSX1 BIOS/BASIC ROM. **Built-in BASIC** in that section
unloads selected ROMs and returns to Weave BASIC in MSX1 mode. With a system ROM selected,
the CPU executes it from reset, allowing original BIOS/BASIC behavior where
compatible. Files are read using `File.arrayBuffer`, never uploaded or persisted.
Do not commit or redistribute user ROMs unless you independently establish permission.
Original proprietary Microsoft/ASCII BIOS/BASIC has not been validated here.

[C-BIOS](https://cbios.sourceforge.net/) 0.29a is bundled for MSX2, with **no original
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

The MSX2 browser tests run the default bundled C-BIOS path at both site roots,
verify real standard-BIOS cartridge pixels, keyboard input and audio/silence, and
check firmware switching, local pairs, privacy and restoration of MSX1. They also
check 512/256-pixel modes, 212 lines, all 256 direct colors and sprite/command pixels.
Screenshots go to `test-results/msx2-screen7.png` and `msx2-screen8.png`.

The Rust tests include exhaustive arithmetic/BCD cases, instruction-family vectors,
interrupt and timing checks, VDP port/mode/sprite/IRQ tests, PSG waveform/envelopes,
slots/PPI, plain and bank-switched cartridge execution, BASIC syntax/runtime tests,
and supplied-ROM boot tests. See [test methodology and hardware references](docs/architecture.md)
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
