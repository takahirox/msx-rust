# msx-rust

An original Rust MSX1 emulator with a browser/WebAssembly frontend, Canvas video,
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

Click the screen and enter:

```basic
10 FOR I=1 TO 5 : PRINT I : NEXT I
RUN
```

The screen prints 1 through 5. `LIST` displays the program. Replace a numbered line
by entering it again, or delete it by entering its number alone. Left/Right, Home,
End, Backspace and Delete edit the current line. ASCII text can also be pasted.
Escape interrupts execution. **Reset** clears the machine and program while keeping
selected ROMs. **Built-in BASIC** also unloads both ROM inputs. **Pause / Start**
controls emulation. There is no persistent program or ROM storage.

Click **Enable audio**, then type `BEEP`. Stop the sustained tone with `SOUND 8,0`.
Audio starts only after a user gesture and may need resuming after browser suspension.
The graphics button draws a sine curve using the emulated VDP; Enter returns to text.

## Hardware and compatibility

| Component | Initial implementation |
| --- | --- |
| Z80 | Main/alternate registers, flags, base/CB/ED/DD/FD/indexed-CB instruction families, stack, I/O, HALT, EI delay, IRQ/NMI, IM 0/1/2 and instruction T-states |
| Memory | Four primary slots, four independently selected 16 KiB pages, 64 KiB RAM in slot 3, 32 KiB system ROM in slot 0, plain cartridge in slot 1 |
| TMS9918A | 16 KiB VRAM, two-write control latch, read-ahead, text/Graphics I/Graphics II/multicolor, 8/16 pixel sprites and magnification, priority, collision/fifth-sprite flags, VBlank IRQ, 256×192 RGBA output |
| AY-3-8910 | Three tone channels, noise LFSR, mixer, amplitude/envelope registers, logarithmic levels, 44.1 kHz mono samples |
| Input/I/O | PPI primary-slot register, keyboard row selection and bit set/reset, international keyboard matrix, PSG joystick 1 input; arrows + Z/X are directions/triggers |
| Browser | Frame accumulator at approximately 59.92 Hz, bounded catch-up, key release on focus loss, Canvas, user-activated Web Audio, local file pickers |

This is an initial implementation, not a claim of universal MSX software compatibility.
Instruction-level timing is used; individual bus phases, MSX M1 wait states, analog
video/overscan, VDP access contention, and undocumented CPU silicon quirks are not
accurately modeled. Sprite status at raster/off-screen edges and unusual combined
VDP modes are not compatibility guarantees. There are no expanded slots, MegaROM
mappers, disk/tape drives, SCC, MSX2 hardware, or advanced peripherals.

## Weave BASIC and firmware

**Weave BASIC is a complete small interpreter for the supported language, not
Microsoft MSX BASIC or a compatible MSX BIOS implementation.** Its original source
is in `crates/msx-core/src/basic.rs`. The bundled Z80 boot ROM polls keyboard events
and invokes this Rust firmware service through private ports F0/F1. The interpreter
writes the real emulated VDP tables and PSG registers; the browser contains no BASIC
parser and does not draw a separate text or graphics mockup. `firmware/boot.asm`
and `crates/msx-core/src/firmware.rs` contain the boot source and exact byte generator.
The service ports are disabled when a user system ROM is selected.

Supported commands: `PRINT` (strings/numbers, `;` and `,`), `LET` or bare assignment,
`IF ... THEN` statement/line number, `GOTO`, nested `FOR ... TO ... STEP ...` / `NEXT`,
`LIST`, `RUN`, `NEW`, `END`, `STOP`, `REM`/apostrophe comments, `HELP`, `CLS`, `SCREEN`,
`COLOR`, `PSET`, `VDP`, `VPOKE`, `SOUND` and `BEEP`. Expressions support parentheses,
arithmetic, powers, comparisons, boolean `AND`/`OR`/`NOT`, `ABS`, `INT`, `SQR`, `SIN`
and `COS`. Variables are case-insensitive, default to zero (empty for `$` strings),
and use f64 numbers. `RUN` clears variables. BASIC execution yields each frame so
Escape remains responsive even with `10 GOTO 10`.

Limitations compared with MSX BASIC: no arrays, `INPUT`, `GOSUB`, `ELSE`, `DATA`,
files, binary tokens, or MSX BIOS entry-point API. Input is ASCII, up to 240 characters
per line, with a horizontally scrolling editor; lowercase glyphs display as uppercase.
There are at most 64 nested loops. Use `PRINT` in text mode. `SCREEN 0` is 40-column
text, `SCREEN 1` is tile graphics, `SCREEN 2` is 256×192 graphics with the TMS9918A
one-foreground/background-pair-per-eight-pixels restriction, and `SCREEN 3` uses
4×4 color blocks. `PSET x,y,color` works in modes 2/3. Color numbers are 0–15.
Low-level `VDP register,value`, `VPOKE address,value`, and `SOUND register,value`
use decimal parameters. `BEEP` sustains a tone until silenced.

For example, enter a numbered graphics program in text mode, then run it:

```basic
10 SCREEN 2
20 FOR X=0 TO 255
30 PSET X,INT(96+60*SIN(X/20)),10
40 NEXT X
RUN
```

## ROM loading and legal guidance

No proprietary BIOS, BASIC or game ROM is included. All bundled code, glyph drawings,
cartridge text/sprites and generated ROM bytes are original and MIT licensed. See
[LICENSE](LICENSE) and the [asset provenance inventory](firmware/ROM-LICENSE.txt).
`./scripts/build.sh` generates both ROMs from source and includes their license notices
in `dist/`. Rust has no external dependencies; Playwright is a development-only dependency
under its upstream license and is not shipped in the site.

Download the original **test cartridge** from the app, then select it in the cartridge
picker. It displays a title and sprite; hold Space to play a roughly 440 Hz PSG tone.
This cartridge runs Z80 code against standard hardware ports, with no native BASIC
service calls. Plain 8, 16, 32 and 48 KiB ROMs with an AB header at 4000h or 8000h are accepted. Small ROMs
mirror in the 4000h–BFFFh window; 48 KiB ROMs start at 0000h. Loading resets the
machine. The small built-in boot ROM initializes self-contained cartridges; many
commercial cartridges need a compatible system BIOS and will not work with it.

Under **Use your own system ROM**, select a lawfully obtained 32 KiB MSX1 BIOS/BASIC
ROM. The CPU executes it from reset, allowing original BIOS/BASIC behavior where
compatible. Files are read using `File.arrayBuffer`, never uploaded or persisted.
Do not commit or redistribute user ROMs unless you independently establish permission.
Original proprietary Microsoft/ASCII BIOS/BASIC has not been validated here.

[C-BIOS](https://cbios.sourceforge.net/) is a separately available BSD-licensed
replacement BIOS suitable for additional cartridge testing. It has **no BASIC**;
it is neither bundled nor the source of our BASIC prompt. C-BIOS 0.29a was tested
as a local input with our cartridge; see the [validation record](docs/validation.md).

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
waveforms connected to a running AudioContext, and uses real file inputs for ROMs.
Screenshots go to the local `test-results/` directory.
Automated audio checks verify the browser signal path, not physical speakers.

The Rust tests include exhaustive arithmetic/BCD cases, instruction-family vectors,
interrupt and timing checks, VDP port/mode/sprite/IRQ tests, PSG waveform/envelopes,
slots/PPI, actual cartridge execution, BASIC syntax/runtime tests, and supplied-ROM
boot tests. See [test methodology and hardware references](docs/architecture.md)
and the [manual smoke flow and actual results](docs/validation.md).

## Deployment

[The Pages workflow](.github/workflows/pages.yml) builds the static site with the
pinned Rust compiler, validates its exact asset inventory, runs browser tests
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
