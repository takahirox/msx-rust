# Local cartridge mapper inference

Issue [#13](https://github.com/takahirox/msx-rust/issues/13) adds independent Rust
content heuristics for Plain, ASCII8, ASCII16, Konami and Konami SCC. No emulator
heuristic source, software database, hashes, publisher/title mappings, proprietary
ROM samples or network requests are used. Test bytes are original MIT-licensed
code. This identifies a plausible memory mapping, not a verified ROM identity or
a successfully running game.

## Method and deterministic scoring

1. Validate each explicit mapping using the existing size, bank/address-range and
   initially visible AB-header rules. Invalid candidates never compete. Size and
   AB alone cannot select a MegaROM mapper.
2. For each valid mapping, trace from the four nonzero AB code entry pointers
   (init, statement, device and text) at 4000h/8000h, within 4000h–BFFFh. Decode
   instruction lengths, including CB/ED/DD/FD operands. Follow direct absolute
   jumps/calls and relative branches; conditional jumps explore both paths.
   Calls follow the callee only: no speculative return state is invented. Stop
   on returns, HALT, indirect transfers, RST, unsupported prefix combinations,
   out-of-window/truncated instructions or 16 consecutive 00h/FFh padding bytes.
   There are at most 8,192 visited states and 64 pending paths per candidate,
   regardless of file size. If any candidate hits a limit, Auto will not infer a
   MegaROM from partial scores. Plain format recognition remains available when
   no banking writes were observed, preserving support for plain data/padding
   images; this does not establish that their entry pointers contain usable code.
   State includes PC, bank selections and known A value.
3. Count reachable `LD (nn),A` instructions (opcode 32h) targeting cartridge
   space. These are probable mapper writes because ROM cannot normally be
   modified, while the supported hardware decodes writes in these ranges as bank
   changes. The [existing mapper table](../README.md#rom-loading-and-legal-guidance)
   describes the precise register windows. Each distinct decoded register window
   contributes **2 points**, or **3** for a write to its first address. Repetitions
   never increase that window's score. Each distinct 2 KiB cartridge address
   region written outside that candidate's registers subtracts **3 points**.
   RAM writes do not count. Shared addresses contribute to every matching mapper.
4. Track an immediate `LD A,n` through NOP, absolute A stores, OUT (n),A, DI/EI and
   unconditional direct jumps; otherwise discard the known value. Known writes
   update a candidate's temporary bank mapping using the real mapper decoder.
   At an unknown-value mapper write, stop that path rather than interpreting an
   invented bank. A direct transfer into a bank physically different from reset,
   followed by at least four non-00h/non-FFh instructions and a return, HALT or
   repeated state, adds **6 points once**. Mirrored bank values do not qualify.
   This structural evidence can distinguish the 8 KiB and 16 KiB strides even
   when both ASCII variants use 7000h. Merely seeing 6000h/7000h, or the absence
   of 6800h/7800h, cannot prove ASCII16.
5. Select a MegaROM candidate only with **score ≥ 6**, **two register windows or
   a supported banked transfer**, and a lead of **at least 4** over the runner-up.
   Ties never select a mapper; hints sort by descending score, then stable ABI
   order (ASCII8, ASCII16, Konami, Konami SCC). Negative scores are allowed.
   If no MegaROM qualifies, a validated plain format is recognized only when no
   reachable cartridge-space A stores suggest unresolved banking. Otherwise
   return an uncertain result with scores and manual-retry guidance.

The detector borrows the ROM and uses bounded tracing storage, without a second
ROM buffer. A selected mapper is validated again before any active machine is
changed. Explicit mapper selection bypasses inference but retains validation.
Failed imports preserve ROM bytes, CPU, RAM, slots, active banks, firmware, model
and peripherals. Successful imports retain the selected model and reset normally.

## Outcomes and browser behavior

`msx_core::cartridge::detect_mapper` returns `Detection`: an optional mapper,
`Confidence`, a list of validated candidates (score, number of register windows,
banked-transfer and truncation flags), and an explanation. Confidence discriminants are:

| Value | Confidence | Meaning |
| --- | --- | --- |
| 0 | Invalid | No supported format passes validation |
| 1 | Insufficient | Too little or conflicting evidence to select a mapper |
| 2 | Ambiguous | A score reaches 6, but selection criteria or margin fail |
| 3 | PlainFormat | Recognized plain size/header without observed banking writes |
| 4 | Likely | MegaROM score and margin pass; still an estimate |

The raw Wasm ABI exposes `msx_detected_mapper()` (existing mapper IDs; 0 means no
selection), `msx_detection_confidence()`, and UTF-8 `msx_detection()` /
`msx_detection_len()` for the last import attempt. Manual imports return their
validated mapper and no heuristic explanation (confidence 0). Transfer/system
imports clear the last-attempt metadata. This metadata is separate from the
unchanged running cartridge after a failure.

Auto is the picker default. Status shows **Auto: mapper, estimate** for MegaROMs
or **Auto: Plain, recognized format**. Failure text includes candidate scores
where available, explains uncertainty, and directs users to choose the documented
mapper in advanced options and open the same file again. Changing the mapper
control alone leaves the loaded cartridge unchanged. ROM contents are never uploaded or persisted. Explicit SRAM imports persist a
local ROM hash and save bytes as described in [cartridge saving](cartridge-sram.md).

## Limits and compatibility

This is a conservative partial code analysis, not a full Z80 program analysis.
It can miss writes through HL/index registers, computed bank values, BIOS-driven
code, indirect dispatch, copied/decompressed RAM code, long initialization paths
or code after calls. Code-like data reachable through an incorrect entry pointer
can still mislead a heuristic. Shared register patterns, generic cartridge-space
writes and unsupported mapper variants can remain ambiguous or be misidentified.
Manual override is available for these cases. Scores are evidence weights, not
statistical probabilities; no universal accuracy is claimed from synthetic tests.

The same bytes produce the same result under MSX1/MSX2; inference never switches
profiles or installs BIOS/sub-ROM firmware. Classic SCC audio is provided by the
selected Konami SCC hardware, not inferred from a game identity. The explicit
ASCII SRAM and R-Type families are documented in [cartridge hardware](cartridge-sram.md);
Auto never selects those families. Known immediate ASCII register writes asserting
a possible SRAM line suppress a ROM-only guess and request a manual choice. ROM
bytes cannot prove SRAM capacity/wiring. SCC+/SCC-I, disks and extended mappers
remain unsupported; inference cannot establish which
BIOS services a cartridge needs. Correct banking alone does not establish
cartridge boot compatibility with the selected hardware.

## Verification

`crates/msx-core/tests/detection.rs` covers plain layouts, all inferred mappers,
actual last-bank CPU execution in both profiles, small banked images, 2/4 MiB
images, ties/conflicts, noncanonical register addresses, repeated generic writes,
skipped data, instruction operands/prefixes, mirrored values, unknown banks,
bounded loops, malformed formats, manual overrides and active-state preservation.
`tests/browser.spec.js` imports original fixtures through the default Auto file
picker and compiled Wasm, checks mapper labels, backdrop pixels and keyboard
responses after failed imports, retries manual overrides, and runs MegaROMs in
MSX2 at both `/` and `/msx-rust/`. Existing reset/eject, firmware, memory-failure,
model-switching and UI tests remain part of the suite.

Local validation on 2026-10-17 used macOS arm64, pinned Rust 1.99.0, Node.js
24.12.0 and Playwright 1.64.1 / Chromium. Results:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 59 integration tests passed, including 7 new detection cases |
| `./scripts/build.sh` | Passed with the [documented local macOS linker wrapper](deployment.md#pre-merge-validation) |
| `node scripts/check-dist.mjs` | Passed; unchanged asset inventory and new metadata exports verified |
| `npm ci`; `npx playwright install chromium` | Passed |
| `npm test` | All 34 Chromium tests passed |
| JavaScript syntax; `git diff --check` | Passed |

The initial build used the host Homebrew compiler without a Wasm target; selecting
Rust 1.99.0 in PATH exposed the already documented missing `libLLVM.dylib` linker
problem. The ignored wrapper fixed it without repository/toolchain changes.
`test-results/auto-ascii16.png` was generated and inspected: the loaded fixture,
Auto estimate and manual controls are visible. Existing screenshots regenerate
with the suite. No proprietary ROMs or external BIOS files were used. These results
do not measure accuracy across real cartridge catalogs.

Required post-merge verification remains **pending**: after Pages publishes the
merged build, record the actual deployment run/commit and published URL, import
an independently authored MegaROM using Auto and record its execution. Local
validation does not establish publication. This check is not a pre-merge gate.
