# Issue 53 WD2793 readiness and verification

Source: [Correct WD2793 Type-I readiness and verification sequencing](https://github.com/takahirox/msx-rust/issues/53).
Local pre-merge validation: 2026-10-24, macOS arm64, pinned Rust 1.99.0,
Node.js 24.12.0, Playwright 1.64.1 / Chromium. The assigned checkpoint was
`01da0b3`; its controller matched evaluation baseline `21f003b`. The live Issue
and comments were read before implementation. This node performed the checks
below; no additional software inputs were needed.

The shared controller now separates READY-independent movement, 30 ms settling
under an HLT-high assumption, and rotational ID/index search. Only selected A
receives mechanical pulses and exposes its sensors. The Philips reset latch
selects A with motor off. V=0 movement timing and sector-transfer/write safety
remain intact. [The supported profile](disks.md) publishes the synthetic ID
positions, rounding, index phase, event ordering and no-input BUSY inference.

Twenty native register tests cover all Type-I families, T/V values, all rates,
no-step cases, both geometries, mismatched tracks, missing sides, diskless and
motor-off A, absent B/no-drive, selection during motion/search, RESTORE exhaustion,
and cancellation during movement/settling/search. Assertions immediately before,
at and after events use independently calculated millisecond deadlines and a
literal nine-position ID table, without reading production movement/verification
timing constants or private head state. Large, irregular and single-cycle
partitions agree, including multi-revolution search and exact settling/ID/index coincidence: 19 tracks at
30 ms plus 30 ms settling completes at precisely 2,147,727 cycles (600 ms),
not 2,147,728 after independently rounding the two phases. At search start this
index counts toward the five-event timeout; a matching ID at the fifth index wins.
Later sector reads infer A's retained position. Existing partial-write, protect,
dirty-media and sector-abort regressions remain in the full suites.

The original `philips-readiness.hex` AB ROM runs through the existing CPU-executed
system fixture without a disk, seeks to track 2, checks no-input verification stays
BUSY beyond one second, then cancels and paints a white checkpoint. Inserting an
image resumes ordinary polling without resetting CPU/head. It checks delayed
no-step verification, reads track 2, writes the independent eight-byte trace
`84 02 80 81 81 BF 80 00` to track 2/sector 9 and paints green only on success.
The exact expected full export preserves every other byte. Native tests vary eight
insertion phases on both models. Compiled Wasm executes the same ROM on both
models at `/` and `/msx-rust/`. The browser pauses at the white checkpoint only to
mount and unprotect the private image without a host-input race. No BIOS traps,
proprietary ROMs, original hardware or gameplay are prerequisites.

Baseline sensitivity was measured by temporarily substituting only baseline
`disk.rs` and `lib.rs`, restoring both corrected sources in a `finally` block.
The final register suite fails 11 tests and passes 9 against the baseline; the
new Philips CPU fixture also fails (diskless PC=45BBh). Examples include leaked
A zero/protect feedback (`C4h` instead of `80h`) on an absent drive, Track remaining
0 instead of reaching 5 without READY, and immediate verification IRQ where BUSY
must remain asserted. The corrected focused suites pass 20 register and 8 Philips
tests, including the original movement/sector fixtures.

| Check | Actual result |
| --- | --- |
| Baseline sensitivity | Expected failures: 11 register tests and 1 CPU fixture |
| Corrected focused native suites | Passed: 20 register and 8 Philips tests |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed: 219 tests, none failed or ignored |
| `./scripts/build.sh` | Passed with pinned toolchain and existing local linker workaround |
| `node scripts/check-dist.mjs` | Passed: inventory, provenance, hashes, subpath links and import-free Wasm |
| `npm ci`, `npx playwright install chromium` | Passed |
| Focused compiled-Wasm readiness fixture | Passed: all 4 model/root combinations |
| `npm test` | Passed: all 114 cases in 10.1 minutes, none failed or skipped |
| Fixture regeneration, JS syntax, `git diff --check` | Passed: six fixtures reproduce; five earlier fixtures unchanged |

The initial build failed because this host's pinned `rust-lld` could not resolve
`libLLVM.dylib`. The successful build uses the previously documented task-local,
ignored wrapper setting `DYLD_LIBRARY_PATH` to the same pinned sysroot's `lib`
and executing its unchanged `rust-lld`; only the per-command
`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER` selects it. The workaround kept
Rust 1.99.0 and changed no tracked build configuration or host toolchain files.
Preliminary native checks used the host Cargo before selecting README's pinned sysroot-first PATH;
all final checks use Rust 1.99.0.

The first focused browser attempt failed on the test's disabled Read-only control
before mount (one timeout, one interrupted case, two unperformed). The next run
passed one and failed three because the fixture's polling bound omitted five MSX
opcode-fetch waits per loop (42 cycles, not 37). Pausing for mount/unprotect and
calculating the bound with those waits fixed the fixture; all four final cases
pass. Native tests now vary insertion phase to exercise that timing range.
Earlier generated fixtures are byte-for-byte unchanged and the new fixture is
original MIT source generated by `tests/fixtures/philips.py`.

Source-Issue/full-diff self-review found no blocking code findings. The review
checked selected-drive mechanics/status, live READY, event rounding/order,
phase cancellation, unchanged sector/write behavior, fixture provenance and
documentation against every acceptance item. All mandatory checks passed; none
were skipped or left unperformed. The full browser run retains MSX1/MSX2
video/input, PSG/SCC/SCC-I audio, firmware/sub-ROM/mapper coexistence, SRAM
persistence and private disk ownership/export coverage alongside the four new
readiness cases. Optional timestamped upstream execution/trace comparison,
physical hardware, proprietary software and commercial
gameplay checks were not performed; authored diagnostics establish this selected
controller contract, not commercial compatibility. Full HLD/status-bit-5 lifecycle,
physical spin-up, flux/CRC anomalies, sector-transfer timing fidelity and additional
commands remain explicitly separate. Issue #53 requires no post-merge verification.
No push, PR, merge or GitHub comment is part of this node.

# Issue 47 WD2793 incremental movement

Source: [Correct WD2793 Type-I per-step movement, timing and interruption](https://github.com/takahirox/msx-rust/issues/47).
Local pre-merge validation: 2026-10-24, macOS arm64, pinned Rust 1.99.0,
Node.js 24.12.0 and Playwright 1.64.1 / Chromium. The assigned upstream checkpoint
was `49a55ca`; its controller matched evaluation baseline
`843a0861af7818e4a28405f039a81f984576a58d`. The live source Issue and its comments
were read before implementation. All checks below were run by this node.

The eight original `disk_type_one` tests all fail with the baseline controller.
Examples: after 30,000 cycles a 0→5 SEEK has already finished at Track 5 instead
of remaining BUSY at Track 2; STEP-IN leaves Track 5 until completion instead of
immediately issuing its first pulse to Track 6; a 79-track seek is already idle
one cycle before the required final deadline. The separate CPU-executed Philips
probe fails with marker EEh instead of A5h (PC=813Eh), because it never observes
the required intermediate Track 2. These baseline failures were reproduced by
temporarily substituting only the baseline controller source, restoring the
corrected source in a `finally` block, and rerunning both focused test binaries.
The corrected controller passes all eight traces and seven Philips tests.

The independent oracle uses `ceil(3,579,545 × milliseconds × intervals / 1000)`;
no production movement constants or private physical-position fields are read.
For a synchronized SEEK with V=0, the independently calculated completion cycles
are:

| Rate | 0 tracks | 1 track | 5 tracks | 79 tracks |
| --- | ---: | ---: | ---: | ---: |
| 00 (6 ms) | 0 | 21,478 | 107,387 | 1,696,705 |
| 01 (12 ms) | 0 | 42,955 | 214,773 | 3,393,409 |
| 10 (20 ms) | 0 | 71,591 | 357,955 | 5,655,682 |
| 11 (30 ms) | 0 | 107,387 | 536,932 | 8,483,522 |

Traces inspect the first pulse at command start and immediately before, at and
after each subsequent SEEK event, including final BUSY/IRQ transitions. They
cover every rate, distances 0/1/5/79 in both directions, sensor-driven RESTORE,
all STEP/T variants and remembered directions, head/register offsets, both drive
boundaries, invalid-sector rejection, interruption between pulses and subsequent
sector addressing. Large, irregular and single-cycle tick partitions retain the
same final position and exact completion boundary without accumulated rounding.
Abort, reset, mount/eject, direct media loss and Philips profile-removal tests
check cancellation and existing lifecycle behavior. Existing verification-result,
partial-write, protection and Weave firmware boot regressions remain passing.

The same original generated Philips sector runs through normal AB-ROM boot and
real Z80 register accesses in native and compiled-Wasm tests. It observes Track 2
and BUSY during a 0→5 seek, checks active-low signals, forces an IRQ, acknowledges
it, waits beyond the original completion, reads all of track 2/sector 1 and writes
a trace to track 2/sector 9. The expected exported image changes only that sector's
first eight bytes to `02 01 FF 02 BF 00 FF 02`. Both models at `/` and `/msx-rust/`
produce green Canvas output and byte-for-byte expected exports. The fixture source
and new hex are original MIT code; regeneration preserves the three older hex
fixtures unchanged. See [the supported timing and boundary policy](disks.md).

| Check | Actual result |
| --- | --- |
| Baseline `disk_type_one` | Expected failure: 8 failed |
| Baseline Philips CPU movement probe | Expected failure: 1 failed |
| Corrected focused `disk_type_one` / `philips` | Passed: 8 / 7 tests |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed: 186 tests, none failed or ignored |
| `./scripts/build.sh` | Passed with pinned toolchain and local linker workaround below |
| `node scripts/check-dist.mjs` | Passed: inventory, hashes/notices, provenance, subpath links and import-free Wasm |
| `npm ci`, `npx playwright install chromium` | Passed |
| Focused compiled-Wasm movement probe | Passed: 4 cases, both models and roots |
| `npm test` | Passed: all 102 cases in 9.3 minutes, none failed or skipped |
| Fixture regeneration, JS syntax and `git diff --check` | Passed; four fixtures reproduce, three existing fixtures unchanged |

Every Cargo command selects Rust 1.99.0 using `rustup run 1.99.0` or README's
pinned sysroot-first PATH. The initial Wasm build failed because this host's
pinned `rust-lld` could not locate `libLLVM.dylib`. The successful build uses the
previously documented ignored linker wrapper: set `DYLD_LIBRARY_PATH` to the
pinned sysroot's `lib` and execute that same sysroot's `rust-lld` with unchanged
arguments. Only `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER` selects the wrapper;
tracked build configuration and host toolchain files are unchanged. The first
browser probe attempt timed out in four cases because its test tried to uncheck
the disabled Read-only control before mounting media. Moving that test action
after mount fixed all four cases; no product change was needed.

Source-Issue/full-diff self-review found no blocking findings. All mandatory
checks passed; none were skipped or left unperformed. The full browser run includes
the existing 98 cases for Weave boot, private disk ownership/export, dirty-media
protection, firmware/sub-ROM/mappers, SRAM persistence, video, audio and input,
plus four movement probes. Production changes are limited to the shared controller
movement engine; both bus decoders and sector-transfer timing retain their existing
behavior. Optional upstream timestamped trace execution/comparison, physical
hardware, proprietary software and commercial gameplay were not performed. These
authored diagnostics establish the supported movement contract, not broad software
compatibility. Verification
settling/search, READY-independent Type I, head-load fidelity and exact internal
startup latency remain explicitly excluded as requested. Required post-merge
verification: none. No push, PR, merge or GitHub comment is part of this node.

# Issue 43 stack and exchange access order

Source: [Correct Z80 stack and EX (SP) memory-access ordering](https://github.com/takahirox/msx-rust/issues/43).
Local pre-merge validation: 2026-10-24, macOS arm64, pinned Rust 1.99.0,
Node.js 24.12.0 and Playwright 1.64.1 / Chromium. The Run's upstream checkpoint
was `eedca0c`; its CPU implementation was unchanged from evaluation baseline
`0ea455fc9dabd42be6e7abeafe2c8305f844b8b2`. A read-only check of GitHub `main`
also returned that baseline. All results below were executed by this implementation
node; no delegated or historical CI results are substituted.

Before changing CPU code, the new `stack_order` suite failed four ordering tests
(PUSH, CALL/RST, interrupt entry and EX); both ordinary-store and POP/RET controls
passed. The new MSX2 PUSH and EX integration tests also failed against the unchanged
CPU: underlying FFFEh RAM became 5Ah, expected retained 6Bh, when high byte 3Fh
hid RAM. Seven other `ram_mapper` tests, including the new MSX1 control, passed.
The combined baseline invocation stopped after the failing `ram_mapper` binary;
`stack_order` was then run separately to obtain its actual failures.

With the correction, PUSH writes FFFFh before FFFEh from SP=0000h. The integration
vectors verify both visible→hidden (retain 6Bh) and hidden→visible (write 5Ah)
transitions, selector readback and the untouched byte behind FFFFh via a page-2
RAM alias. All HL/IX/IY exchanges at SP=FFFEh retain their original two-byte read
result even when the subsequent high-byte write changes the slot. The MSX1 probes
verify that FFFFh remains ordinary flat RAM.

| Check | Actual result |
| --- | --- |
| Baseline `cargo test -p msx-core --locked --test stack_order` | Expected failure: 4 failed, 2 controls passed |
| Baseline `cargo test -p msx-core --locked --test stack_order --test ram_mapper` | Expected failure in `ram_mapper`: 2 failed, 7 passed; `stack_order` run separately above |
| Corrected focused command with both test binaries | Passed: 6 access-trace and 9 RAM-mapper tests |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed: 165 tests, no failures or ignored tests |
| `./scripts/build.sh` | Passed with pinned toolchain and local linker workaround below; first attempt failed to load `libLLVM.dylib` |
| `node scripts/check-dist.mjs` | Passed: exact inventory, disk assets, pinned ROM/notice hashes, provenance, subpath links and import-free Wasm |
| `npm ci`, `npx playwright install chromium` | Passed |
| `npm test` | Passed: all 98 browser tests in 9.2 minutes, no failures or skipped tests |

The shell initially selected Homebrew Rust 1.94.0; every Cargo validation command
above used the README's Rust 1.99.0 `PATH` selection. The pinned toolchain's
`rust-lld` could not locate its existing `libLLVM.dylib`. As in the earlier envelope
validation below, an ignored `target/issue43-rust-lld` shell wrapper exports
`DYLD_LIBRARY_PATH` as the pinned sysroot's `lib` directory and executes that
sysroot's `lib/rustlib/aarch64-apple-darwin/bin/rust-lld` with its original arguments.
Only the build invocation selects the wrapper through
`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER`; tracked build configuration and host
toolchain files are unchanged.

The [architecture contract](architecture.md#cpu-memory-access-order) documents
ordered accesses and the remaining instruction-level timing approximation.
Source-Issue/full-diff self-review found no blocking findings. Production changes
are limited to the shared CPU's PUSH and EX write sequences; the low-first word
helper, instruction cycle totals and bus/peripheral implementations are unchanged.
Optional pinned reference-emulator trace comparison, physical hardware,
proprietary firmware and commercial gameplay were not performed. These tests do
not establish broad commercial compatibility. Required post-merge verification:
none. No push, pull request, merge or GitHub comment is part of this node.

# Issue 37 AY envelope

Source: [Correct AY-3-8910 envelope timing and register-write phase behavior](https://github.com/takahirox/msx-rust/issues/37).
Local pre-merge validation started 2026-10-23 on macOS arm64 with pinned Rust
1.99.0, Node.js 24.12.0 and Playwright 1.64.1 / Chromium. The [timing and write
contract](psg-envelope.md) records exact timestamps, original fixtures and evidence
limits. Full validation completed on 2026-10-24.

| Check | Actual result |
| --- | --- |
| Focused cycle-32 regression against unchanged baseline PSG | Failed as required: level 15, expected 14; passed with corrected PSG |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed: 147 Rust tests, including 12 new envelope tests; no failures or ignored tests |
| `./scripts/build.sh` | Passed with pinned toolchain and local linker environment workaround below |
| `node scripts/check-dist.mjs` | Passed: exact distribution inventory, pinned hashes/notices/provenance, subpath links and import-free Wasm |
| `npm ci`, `npx playwright install chromium` | Passed |
| `npm test -- tests/envelope.spec.js` | Passed: four model/hosting combinations, real Wasm PCM and scheduled Web Audio intervals of 1621–1622 samples |
| `npm test` | Passed: all 88 browser tests in 8.4 minutes, including existing firmware, disk, SCC lifecycle and SRAM checks |

The initial shell selected Homebrew Rust 1.94.0, which lacked its Wasm target.
Selecting the README's pinned Rust 1.99.0 fixed that; its installed `rust-lld`
then failed to find `libLLVM.dylib`. An ignored `target/` linker wrapper sets
`DYLD_LIBRARY_PATH` to the pinned toolchain's `lib` directory immediately before
executing that toolchain's `rust-lld`; only the build process selects it with
`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER`. Setting the library path outside
`build.sh` alone did not survive the local shell/process environment. No tracked
build configuration or host toolchain files were changed. Mandatory Rust checks
used 1.99.0; the initial baseline diagnostic used the shell's 1.94.0.

The first browser probe run passed three combinations and failed its scheduled
buffer continuity assertion in one. That assertion assumed buffer
continuity, which the scheduler does not guarantee during catch-up. It now records
frame identities and measures only contiguous scheduled frames; the corrected
four-case focused run passed. Deterministic twenty-frame Wasm PCM checks also
pass and do not depend on real-time scheduling.

Source-Issue/full-diff self-review completed with no blocking findings. Production
changes are limited to the shared envelope state/counter; public APIs, tone/noise
clocks, sample buffering, frontend lifecycle and firmware assets are unchanged.
No push, PR, merge or GitHub comment is part of this node. Optional pinned openMSX
trace comparison, commercial-title listening, speaker tests and physical AY
measurements were not performed. The
zero-period and write-phase rules remain provisional, partly based on YM2149
evidence. Required post-merge verification: none.

# Issue #33 MSX opcode-fetch wait validation

Source: [Account for MSX opcode-fetch waits while preserving standalone Z80 timing](https://github.com/takahirox/msx-rust/issues/33).
Local pre-merge validation: 2026-10-23, macOS arm64, pinned Rust 1.99.0,
Node.js 24.12.0, Playwright 1.64.1 / Chromium.

| Check | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 130 tests passed; no failures or ignored tests |
| `cargo test -p msx-core --test fetch_timing --locked` | 5 passed after strengthening the command-completion boundary assertion |
| `./scripts/build.sh` | Passed with the pinned toolchain and local linker workaround below |
| `node scripts/check-dist.mjs` | Passed; deployment inventory, firmware/demo hashes, notices, provenance, subpath links and import-free Wasm |
| `npm ci`; `npx playwright install chromium` | Passed |
| `npm test` | 82 passed, 2 failed in YOKO startup assertions; both failures resolved as described below |
| `npm test -- tests/demos.spec.js` | All 7 demo tests passed after the fixture correction, including both previously failing tests at root and project subpath |
| `node --check tests/demos.spec.js`; `git diff --check` | Passed |

The full browser run exposed a fixed eight-frame start-button assumption: the HUD
can become visible before YOKO starts polling input. The test now retries the
start press within a bounded 64 emulated frames and still requires the actual
ship, input-driven movement, Reset/Eject and diagnostic pixels. All seven affected
demo-suite tests passed on rerun; the other browser suites passed in the full run.
The full 84-test suite was not repeated after this test-only correction.

Homebrew Rust initially shadowed rustup and lacked its Wasm target. With the
README's pinned-toolchain PATH selection, rust-lld then failed to locate its own
`libLLVM.dylib`. The existing [local macOS linker workaround](deployment.md#pre-merge-validation)
resolved this: an ignored `target/msx-wasm-linker.sh` exports the pinned sysroot's
`lib` directory as `DYLD_LIBRARY_PATH` immediately before invoking rust-lld;
`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER` selects that wrapper. No toolchain,
CI, clock-frequency, firmware, licensed asset or application pacing changes were
needed.

The [timing contract and original fixtures](architecture.md#cpu-fetch-timing)
cover standalone/MSX instruction counts, interrupts, R-register independence,
bounded prefix yielding, lifecycle policy retention, device boundaries and frame
overshoot. Review of the complete diff against Issue #33 found no outstanding
blocking requirements. Optional openMSX trace comparison and physical hardware
measurement were not performed; these checks establish this timing correction,
not broad commercial-game compatibility. Required post-merge verification for
Issue #33: **None**. No push, PR, merge or comment was performed.

# Issue #17 Weave BASIC compatibility validation

Source: [Expand Weave BASIC compatibility with classic MSX BASIC programming features](https://github.com/takahirox/msx-rust/issues/17).
Local pre-merge validation: 2026-10-20, macOS arm64, pinned Rust 1.99.0,
Node.js 24.12.0, Playwright 1.64.1 / Chromium. No publication or merge was performed.

| Check | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | All 74 integration tests passed, including 17 BASIC tests |
| `./scripts/build.sh` | Passed with pinned Rust and the existing macOS linker workaround below |
| `node scripts/check-dist.mjs` | Passed; exact deployment inventory, C-BIOS hashes/notices, ROM headers, Wasm exports/imports and project-relative links |
| `npm ci`; `npx playwright install chromium` | Passed |
| `npm test` | All 38 Chromium tests passed (1.4 minutes), including both new BASIC paths |
| JavaScript syntax; `git diff --check` | Passed |

Homebrew Rust initially shadowed the pinned toolchain and lacked its Wasm target.
The pinned toolchain's linker then failed to locate its own `libLLVM.dylib`.
Using the PATH selection in README and an ignored `target/weave-wasm-linker.sh`
wrapper exporting the toolchain's library directory fixed both environment issues,
without source/toolchain/CI modifications. This is the same local workaround
already documented in [deployment.md](deployment.md#pre-merge-validation), using
`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER` to select the wrapper.

The independently authored BASIC fixtures cover nested calls and caller loops,
direct and numbered execution, END/STOP/error/BREAK cleanup, repeated RUN, arrays
and bounds/types, ordered DATA and RESTORE, literal quoting, INPUT acceptance,
atomic retry and interruption, and cooperative infinite-program execution.
Graphics tests inspect actual VRAM and rendered VDP pixels in SCREEN 2/3,
including circle outlines, boxes, relative coordinates, clipping, open/same-color
fills and bounded repeated drawing. The three MIT-licensed examples are tested
in Rust and through keyboard events in the compiled browser/Wasm environment.
See [the language guide and public specification record](../README.md#weave-basic-and-firmware)
for supported forms and deliberate deviations.

Browser tests run examples at both `/` and `/msx-rust/`, independently recognize
output glyphs and sample graphics pixels, retry INPUT, retain focus, interrupt
partial input and repeated full-screen PAINT, and reset pending input. Existing
MSX1/MSX2, C-BIOS, local firmware, cartridges, mapper, audio, editing, focus,
fullscreen and responsive-layout regressions are included. Shapes screenshots
are saved to `test-results/basic-shapes-root.png` and `basic-shapes-pages.png`.
No proprietary ROM or third-party interpreter implementation was used.

Required post-merge verification remains pending: after merge and deployment,
record the actual GitHub Pages Actions run and commit, open
<https://takahirox.github.io/msx-rust/>, boot MSX1, and run the scores and shapes
examples with visible output. Local tests do not establish deployment success;
this follow-up cannot block pre-merge approval. File defects as follow-up Issues.

---

# Issue #8 MegaROM validation record

## Integration with the merged Issue #7 UI

Before PR publication on 2026-10-17, integrated `main` at
`9e79661a899823317a63c6a4faa94e148ebff6d3` (PR #9). Preserved the redesigned
toolbar, disclosures, audio controls, fullscreen and focus behavior. Mapper
selection lives under **ROMs & advanced system options**; **Open ROM** still
opens the cartridge picker. Replaced the obsolete size-rejection assertion and
retained both the mapper and UI browser coverage. Eject can cancel a pending
file read without allowing its eventual completion to reload the cartridge.

Re-ran `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo test --workspace --locked`: all passed, including all 34 Rust integration
tests. `./scripts/build.sh` passed with the documented macOS linker wrapper;
`node scripts/check-dist.mjs`, `npm ci`, `npx playwright install chromium`, and
`npm test` passed. The combined browser suite passed all **27 Chromium tests**,
including real banked file imports, allocation-failure recovery, project-prefix
hosting, mobile layouts and fullscreen. JavaScript syntax and whitespace checks
also passed. No workflow changes were needed. External BIOS/game ROMs and other
browsers were not tested. Required deployed Pages verification remains pending
until after merge; local results do not establish deployment success.

## Original implementation validation

Source: [Remove the arbitrary cartridge ROM size limit and support bank-switched MegaROMs](https://github.com/takahirox/msx-rust/issues/8).
Local validation date: 2026-10-17. Pre-merge worktree validation only; no push, PR,
comment, merge or deployment was performed.

Environment: macOS arm64, pinned rustup Rust 1.99.0 (`b940084d7`), Node.js 24.12.0,
Python static servers, Playwright 1.64.1 / Chromium 156.0.8078.4.

| Command / check | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | All 34 integration tests passed: 8 mapper, 13 hardware/machine, 7 CPU, 6 BASIC |
| `./scripts/build.sh` | Release Wasm and site passed with the existing macOS linker wrapper documented in [deployment.md](deployment.md#pre-merge-validation) |
| `node scripts/check-dist.mjs` | Passed: unchanged eight-file asset inventory, ROM headers/sizes, relative paths, import-free Wasm and required transfer exports |
| `npm ci` | Passed; locked development dependencies, no vulnerabilities reported |
| `npx playwright install chromium` | Passed; browser available |
| `npm test` | All 14 Chromium tests passed, including real MegaROM file imports and `/msx-rust/` hosting |
| `node --check web/app.js`; `node --check tests/browser.spec.js` | Passed |
| `git diff --check` | Passed |

The host initially selected Homebrew Rust 1.94.0, which lacked the Wasm target.
Selecting the pinned rustup compiler in PATH resolved that mismatch. Its macOS
linker could not locate `libLLVM.dylib`; an outer DYLD_LIBRARY_PATH did not help.
The repository's already documented ignored `target/pages-linker.sh` wrapper fixed
that environment problem without source, compiler or CI configuration changes.

Eight new Rust tests exercise exact ASCII8/ASCII16/Konami/Konami SCC register
ranges and ignored writes, initial banks, bank wrapping, distinct Konami mirrors,
SCC enable/ROM-overlay decoding, slot visibility, reset, replacement/ejection,
system-ROM separation and transactional validation failures. Synthetic images
from 64 KiB to 4 MiB verify last-bank reads without truncation. Z80 fixtures on
all four mappers switch a bank and execute code from it. Existing plain cartridge,
header/entry-point, keyboard, video, PSG and BASIC regressions also pass.

Five browser cases select original generated 64/128/256/512/1024 KiB MegaROM files
through the real file input and compiled Wasm path. At the time of Issue #8,
Auto requested explicit mapper selection; Issue #13 adds content inference (see
[the current method and validation](mapper-detection.md)). Code in the final
switched bank controls Canvas backdrop pixels and reacts to Space. This remains responsive after malformed-header, partial-image,
wrong-extension and invalid-system-ROM imports. Each case resets, replaces the
mapper, loads a plain cartridge, ejects and returns to built-in BASIC. File imports
cause no upload or third-party requests. The actual `/msx-rust/` server test also
imports and runs a 128 KiB ASCII16 fixture under the project prefix.

A raw Wasm test reserves beyond initial linear memory, creates a fresh writable
view after growth, writes its last byte, and checks invalid transfer lengths,
mapper IDs, zero lengths and an unrepresentable u32 reservation without a trap.
A browser fault-injection test bounds only its test Wasm instance to 4 MiB, then
requests an 8 MiB reservation to exercise a genuine try_reserve_exact failure
without exhausting host memory. It separately simulates File.arrayBuffer allocation
failure; the existing banked cartridge still responds to keyboard input. A deferred file read completed after
Eject cannot reload its cartridge. Actual host-wide memory exhaustion was not
attempted. These fixtures contain original MIT-licensed bytes only, with no external
ROM downloads or additional deployable ROM assets.

At the time of Issue #8, SCC sound/register hardware was not implemented;
[classic SCC support and its validation](scc.md) were added for Issue #23.
SRAM mapper variants and universal game compatibility are not claimed.
Issue #8 originally limited Auto to plain layouts. The current
[Auto detector](mapper-detection.md) also estimates supported MegaROM mappers;
uncertain images still require manual selection. See the [mapper table and compatibility limits](../README.md#rom-loading-and-legal-guidance).
Optional external C-BIOS, third-party games, other browsers and physical audio
listening were not tested for this change. No workflow changes were needed.

## Issue #8 required post-merge verification — pending

After merge and the Pages deployment, verify the actual deployed UI and Wasm at
<https://takahirox.github.io/msx-rust/>. Select a permitted MegaROM with its documented
mapper, observe execution and confirm the obsolete 48 KiB rejection is absent.
Record the deployment run, commit and actual result; file follow-up Issues for
defects. This implementation node does not deploy or perform post-merge checks.
Local build/subpath results do not establish deployed results, and this pending
check must not block pre-merge review or merge.

---

# Issue #3 validation record

Source: [Implement the complete initial browser-playable MSX1 emulator](https://github.com/takahirox/msx-rust/issues/3).
Local validation date: 2026-10-17. This is a pre-merge worktree record, not a claim
that GitHub Actions ran or a public site was published.
For the current UI controls and Issue #7 results, see [UI validation](ui-validation.md).
The original application results below are retained for provenance. For the Pages
scaffold's integration results, see [deployment validation](deployment.md#pre-merge-validation).

## Review fixes and revalidation

On 2026-10-17, integrated `main` at `618911c8eeabd4ab0f653a35f0ec43d19da0637a`
and resolved all seven conflicting files. Retained the pinned Rust compiler,
clean `dist/` build, actual project-prefix static server, deployment guide and
main-only Pages deployment. The combined workflow also retains Rust formatting,
Clippy and tests, plus the `browser-evidence` upload.

Added a deterministic PSG muting regression for each of the three channels:
first require tone samples above 0.05, then write zero amplitude, allow 250 ms
of emulated filter decay, and check every fresh sample over 100 ms is finite
and below 0.0001 absolute amplitude. Earlier samples are discarded.

The browser test activates Web Audio and verifies `BEEP` produces a peak above
0.05 and more than ten positive zero crossings. After `SOUND 8,0`, it allows
500 ms for queued playback (at most approximately 150 ms), filter decay and frame
margin. It then requires at least twelve newly scheduled buffers (about 200 ms)
to finish playback in a running AudioContext, with every sample finite and below
0.0001 absolute amplitude. Per-buffer peaks and completion events prevent stale
tone counters or a stopped scheduler from being mistaken for successful muting.

Revalidated on the macOS/Rust/Node/Chromium environment listed below:

| Command / check | Actual result after conflict resolution |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | All 26 integration tests passed: 7 CPU, 13 hardware/machine, 6 BASIC |
| `./scripts/build.sh` | Initial macOS linker load failed; passed using the documented local wrapper in [deployment.md](deployment.md#pre-merge-validation) with `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER` |
| `node scripts/check-dist.mjs` | Passed; eight-file deployment inventory and Wasm/ROM checks |
| `npm ci`; `npx playwright install chromium`; `npm test` | All seven Chromium tests passed, including tone/muting and actual `/msx-rust/` hosting |
| `go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 .github/workflows/pages.yml` | Passed |
| Build fingerprints | All three hashes match the original application build below |

The optional external C-BIOS tests and human listening were not repeated. No
push, GitHub merge or deployment was performed. Production verification remains
pending; the automated audio checks establish signal generation and muting,
without claiming a physical-speaker listening result.

## Original application environment and completed checks

macOS arm64; rustup Rust 1.99.0 (`b940084d7`), wasm32-unknown-unknown;
Node 24.12.0; Python 3.13.13; Playwright 1.64.1; headless Chromium 156.0.8078.4.
The test suite starts an isolated local HTTP server and refuses to reuse a service
already listening on its port.

| Command / check | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 25 integration tests passed: 7 CPU, 12 hardware/machine, 6 BASIC; no failures or ignored tests |
| `./scripts/build.sh` | Release Wasm and static site built; original boot/test ROMs generated from source |
| `node scripts/check-dist.mjs` | Exact eight-file deployment inventory, ROM sizes/header, relative links and import-free Wasm verified |
| `npm ci` | Locked development dependencies installed; no runtime JS dependencies bundled |
| `npx playwright install chromium` | Browser available |
| `MSX_TEST_BIOS=target/validation/cbios_main_msx1.rom npm test` | Eight browser tests passed, including optional C-BIOS integration |
| `cargo run --release -p msx-core --example check_bios -- target/validation/cbios_main_msx1.rom` | Unmodified C-BIOS booted the authored cartridge; 600 frames, VDP text, PPI Space input and PSG output passed |
| Workflow YAML | Parsed locally; verify/deploy jobs, conditional main-branch deployment, artifact dependency and Pages/OIDC permissions reviewed against GitHub's documented workflow |
| `git diff --check` | Passed after staging final source/documentation |
| Browser screenshots | BASIC prompt/output, graphics curve, built-in cartridge boot and C-BIOS cartridge boot inspected visually |

The default `npm test` suite has seven tests and needs no external ROM. Setting
`MSX_TEST_BIOS` adds the eighth test; it is not fetched by the tests or used by CI.
Screenshot evidence is regenerated in `test-results/basic.png`, `graphics.png`,
`cartridge.png`, and (when opted in) `cbios-cartridge.png`. CI uploads that directory.

Browser checks actually performed:

- Booted the compiled Wasm to an interactive BASIC screen, typed/stored/listed/ran
  `10 FOR I=1 TO 5 : PRINT I : NEXT I`, and recognized 1–5 directly from Canvas pixels.
- Replaced/deleted numbered lines and edited a command with Home/End/arrows/Backspace.
- Paused/resumed/reset the machine and interrupted an infinite BASIC program with Escape.
- Rendered the graphics sine curve through Graphics II VRAM and checked its colored pixels.
- Unlocked a real AudioContext via a button, issued `BEEP`, and observed nonzero,
  oscillating samples in AudioBufferSources connected to the running audio destination.
- Loaded our cartridge through the file picker, saw its text/sprite, rejected an
  invalid ROM without replacing it, and ejected back to BASIC. The Rust cartridge test
  separately verified Space-controlled PSG amplitude and nonzero samples.
- Loaded an original authored system-ROM fixture through the system picker, observed
  its VDP output, verified native BASIC service ports were disabled, and returned to BASIC.
  File loading caused no upload/non-GET or third-party network requests.
- Booted an unmodified, locally supplied C-BIOS with our cartridge in the actual browser.
- Pasted a program longer than the core input FIFO and verified its final line/output.
- Loaded the same artifact under a simulated `/msx-rust/` project prefix, with its
  contents unchanged, and typed a BASIC command successfully.

Build fingerprints from the validated local build (compiler-dependent, not release signatures):

```text
msx_wasm.wasm      3d41eb3a179434b0369614716d5446634ef3bc40647267bbb8c654f40998779e
test-cartridge.rom 118c6242a2071a4ad1f4e63a52b86bd9625420b6aa1b41b9e8690fd203f2278d
weave-boot.rom     a49cc03789a09b0a395b01756925903a1ac4f5f31b4fc1a3747f2a4c6b89d079
```

## Reproduce optional C-BIOS verification

The [C-BIOS Association](https://cbios.sourceforge.net/) explicitly identifies its
BIOS as BSD-2-Clause and states it contains no BASIC. The license notice was reviewed
before obtaining the archive. Only the ROM was executed; no C-BIOS source was used
to implement the emulator. The archive/ROM remain under ignored `target/validation/`
and are absent from committed sources and the deployable artifact.

```sh
mkdir -p target/validation
curl -fL https://downloads.sourceforge.net/project/cbios/cbios/0.29/cbios-0.29a.zip -o target/validation/cbios-0.29a.zip
shasum -a 256 target/validation/cbios-0.29a.zip
# Expected: c585a2f4a10b397e460a95043d59ac79cf4442fbb45a6e14a1e27ef9febaed73
unzip -jo target/validation/cbios-0.29a.zip 'cbios-0.29a/roms/cbios_main_msx1.rom' -d target/validation
cargo run --release -p msx-core --example check_bios -- target/validation/cbios_main_msx1.rom
MSX_TEST_BIOS=target/validation/cbios_main_msx1.rom npm test
```

## Manual browser smoke flow

1. Build/serve using the README commands. Confirm the visible blue display says
   `WEAVE BASIC 0.1` and `READY`. Click the display before typing.
2. Type `10 FOR I=1 TO 5 : PRINT I : NEXT I`, Enter, `LIST`, Enter, `RUN`, Enter.
   Confirm visible lines 1–5. Replace line 10 with `10 PRINT 42` and run again.
   Enter `10` alone and `LIST` to verify deletion. Exercise arrow/Home/End editing.
3. Click **Graphics demo** and confirm a yellow curve appears. Press Enter to return.
   Optionally run the numbered graphics example from README.
4. Click **Enable audio**; type `BEEP` and listen for a steady tone. Type `SOUND 8,0`
   and confirm silence. Audio should remain silent before user activation.
5. Expand **ROMs & advanced system options**, download `test-cartridge.rom`, then
   choose it with **Open ROM**. Confirm title and yellow sprite. Click the display and hold Space: hear the tone, then release to silence. Check that focus
   loss releases held keys. Reset should boot the cartridge again; Eject returns to BASIC.
6. Optionally select your lawfully obtained 32 KiB system ROM, then a compatible
   plain cartridge. Files must stay local. **Built-in BASIC** unloads both inputs.
7. Pause, confirm video execution stops, and Resume again. Reload to verify the
   original BASIC prompt returns without retaining user ROM data.

## Limitations and unperformed checks

The implemented built-in interpreter covers the required language constructs and
VDP/PSG demos, but is a Rust firmware service, not a Z80 Microsoft BASIC implementation
or a full MSX BIOS API replacement. Self-contained plain cartridges and C-BIOS boot
were verified. Original proprietary BIOS/BASIC and third-party games were not tested
or obtained; compatibility beyond the supplied software is not established.

CPU tests are deterministic and cover the implemented prefix families, but are not
exhaustive Z80 silicon conformance. Bus phases/M1 wait states, undocumented quirks,
analog video/audio fidelity, and raster-edge behavior remain approximate. Out-of-scope
mappers, expanded slots and peripherals are listed in README. No reference-emulator
or physical-MSX comparison, Firefox/Safari/mobile testing, or physical-speaker
listening was performed. Automated Web Audio waveform/destination verification passed;
manual listening remains unperformed, not reported as passed.

Early checks exposed a Homebrew/rustup toolchain mismatch and an unrelated server on
the original test port; toolchain selection and isolated-server configuration fixed
them. Canvas digit recognition was tightened to exclude program-listing prefixes.
Graphics review found multicolor addressing, tile-mode CLS and transparent-sprite
collision issues; fixes have regression coverage. No failing checks remain in the
reported final automated suites.

## Required post-merge verification — pending

Target: <https://takahirox.github.io/msx-rust/>. No push, PR, merge, comment or deployment
was performed by this implementation node. A read-only `GET /repos/takahirox/msx-rust/pages`
returned HTTP 404; current publication/configuration could not be established.

Ensure repository **Settings → Pages → Source** is **GitHub Actions**, and the
`github-pages` environment permits deployment from `main`. After merge and a successful
workflow, record the actual published URL and repeat the smoke flow against that URL,
including keyboard interaction, local ROM selection, and audio activation/listening.
Check that Wasm and all relative assets load without errors. Pre-merge artifact and
subpath checks do not establish that publication succeeded. Keep this pending until
performed; do not close it based on a local build alone.

## Issue #49: ROMless SCC-I sound cartridges (2026-10-24)

Implemented explicit Snatcher and SD Snatcher 64 KiB profiles across the core,
raw Wasm ABI and advanced browser controls. See [hardware decoding, reset policy,
provenance and fixture methodology](scc.md#romless-snatcher-and-sd-snatcher-scc-i).
The original CPU fixture loads a sector through the Philips interface, executes
code copied into accessory RAM, checks compatible/plus fifth-wave readbacks and
writes independently expected disk bytes. Native/browser cases cover both models
and profiles; browser cases run at `/` and `/msx-rust/` with compiled Wasm.

| Check | Final result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed, 198 tests |
| `./scripts/build.sh` | Passed using the documented local linker workaround |
| `node scripts/check-dist.mjs` | Passed, including new exports and asset provenance |
| `npm ci` / `npx playwright install chromium` | Passed |
| `npm test` | Passed, 110 cases, 9.7 minutes; includes eight SCC-I cases and existing SCC/PSG, SRAM, disk and firmware regressions |
| `git diff --check`, staged diff whitespace and local Markdown file links | Passed |

Used the pinned Rust 1.99.0 toolchain at the front of PATH. The initial Wasm link
failed with the already documented macOS `libLLVM.dylib` lookup issue; the ignored
`target/local-linker/rust-lld` wrapper sets that toolchain's library path and invokes
its original linker. No dependency, toolchain or production build settings changed.
Development fixture/bootstrap errors were corrected. Earlier browser attempts were
intentionally interrupted for a test setup correction and then a reference-review
fix; the final full run above completed without failures or skipped cases. The
final build/inventory checks were repeated after a help-text clarification.

Reviewed the complete source Issue and staged diff using the repository review
guidelines. Review included both RAM populations, selector/protection boundaries,
mode priority, SCC-I maps and deformation, independent fifth-wave PCM, hidden
phase continuity and PSG mixing, allocation failures, reset versus insertion,
slot-1 replacement, preserved firmware/modified disks, browser cancellation and
outgoing SRAM saves. A raw deformation-bit clock-source detail in the pinned
reference was corrected and tested with distinct channel phases. No blocking
findings remain in the local source/full-diff review. No PR was opened or reviewed,
and no push, comment or merge was performed.

All mandatory local checks were performed and passed. Optional openMSX runtime/PCM
comparison and physical measurements were unperformed. Underlying RAM writes
beneath readable sound overlays remain the explicitly provisional upstream policy;
DAC latency, analog effects and deformation timing retain documented synthesis
approximations. No proprietary firmware or commercial-game bytes were used.
Fixture success does not establish Snatcher or SD Snatcher execution/playability.
Issue #49 requires no post-merge verification; publication was not performed.
