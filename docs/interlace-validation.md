# V9938 field timing validation — Issue #71

Implementation and source-Issue review: 2026-10-24, based on
`8aaeffdf863896cc56d1f80e5af2c09080124761` plus the GitWeave readiness checkpoints.
See [the timing model](msx2.md#field-timing-and-r9-fixation) for field origins,
R9 fixation, active display, interrupts and consumer contracts.

## Regression evidence

Before editing production code, the first two new tests in
`crates/msx-core/tests/interlace.rs` were run against the unchanged core:

```text
cargo test -p msx-core --test interlace --locked
literal_field_boundaries_and_continuous_horizontal_phase: FAILED
  frame_cycles(): actual 59736, expected 59850
hundred_fields_and_irregular_partitions_have_literal_totals: FAILED
  at 5,984,999 T-states: actual 100 fields, expected 99
```

The corrected tests cover NTSC/PAL and both 192/212-line configurations. Their
100-field endpoints are 5,985,000 and 7,125,000 T-states, with 99 fields one
T-state before either endpoint. Bulk ticks and irregular partitions agree,
including half-lines carried through multiple field boundaries. Every woven row
is checked across four fields with changed VRAM contents, both EO settings and
384/424-row displays, including preservation of the other parity's rows.

The Machine diagnostic executes an 11-T RAM JP loop. At 100 interlaced fields:

| Timing | Budget total | Actual CPU T-states | Instruction overshoot | Cumulative PCM samples |
| --- | ---: | ---: | ---: | ---: |
| NTSC | 5,985,000 | 5,985,001 | 1 | 73,735 |
| PAL | 7,125,000 | 7,125,008 | 8 | 87,780 |

Further calls change timing repeatedly while preserving the same cumulative
debt/sample equations and independent FH/VBlank acknowledgements. Existing
fetch/device, disk, cassette, RTC, command, PSG/SCC/FM and lifecycle suites remain
in the full checks.

The original Z80 port/status probe records 35 field-edge observations, including
both parities and NTSC IL → PAL IL → PAL → NTSC transitions. Its timestamped trace
and reproduction command are in [the diagnostic description](msx2.md#original-diagnostics-and-evidence).
The browser runs that probe through real firmware inputs and compiled Wasm at
both site roots, observes Canvas weaving and height/budget transitions, and
checks PCM totals, fresh Web Audio buffer lengths and the actual animation-frame
accumulator against independent
literal budgets over more than 100 frames. No special debug ABI was added.

## Environment and checks

Used Rust 1.99.0, Node 24.12.0, `npm ci` and `npx playwright install chromium`.
The initial build failed because Homebrew Rust 1.94.0 shadowed rustup and could
not find the Wasm target. The documented README PATH setup selected the pinned
compiler; `rustup target add wasm32-unknown-unknown` confirmed its target was
already installed.

A subsequent build exposed this host's `rust-lld` loader failure for
`@rpath/libLLVM.dylib`. The library exists in the pinned toolchain's `lib/`.
An ignored `target/issue-71-linker/rust-lld` shell wrapper sets
`DYLD_LIBRARY_PATH` to that directory and execs the same toolchain's
`lib/rustlib/aarch64-apple-darwin/bin/rust-lld`, forwarding all arguments.
`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER` points to that wrapper for the
build only. This is a local macOS toolchain workaround; no tracked build script,
compiler version, dependencies or emulator behavior were changed for it.

| Required check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed: 299 tests, no failures or ignored tests |
| `./scripts/build.sh` | Passed with the pinned PATH and local linker wrapper above |
| `node scripts/check-dist.mjs` | Passed: inventory, hashes, provenance, links and import-free Wasm |
| `npm test` | Passed: 202 tests in 15.0 minutes, no failures or skipped tests |
| `git diff --check` | Passed |

The targeted compiled-Wasm interlace suite passed both roots before the full
browser run and again after final review strengthened Web Audio buffer and
repeated-weaving assertions (`npx playwright test tests/interlace.spec.js`: two
passes, 4.0 seconds). The final native Machine sample assertions and updated old
weave fixture were also rerun successfully. The two expected baseline regression
failures and the initial local build/toolchain failures above were corrected;
they are not successful checks.
Source-Issue and full-diff review checked field/line ordering, all active R9
consumers, IRQ acknowledgement, instruction debt and browser entry-budget
accounting. Review updated the old weave fixture's whole-line durations and the
incompatible immediate PAL → NTSC transition expectation. It also bounded the
browser diagnostic's animation drive and extended weaving checks through repeated
timing cycles. All mandatory checks passed and no blocking review finding remains.
No mandatory check was skipped.
Optional hardware recordings and pinned-emulator comparisons were unperformed;
no commercial-title failure, playability or physical-silicon equivalence is claimed.
The source Issue requires no post-merge verification. Nothing was pushed,
commented, merged or opened as a PR.
