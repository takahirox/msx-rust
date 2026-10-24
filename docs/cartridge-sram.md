# ASCII cartridge SRAM and R-Type

Issue [#27](https://github.com/takahirox/msx-rust/issues/27) adds explicit hardware
choices to the Rust core, raw Wasm ABI and browser. IDs 0–5 keep their existing
meaning; 6 = ASCII8SRAM2, 7 = ASCII8SRAM8, 8 = ASCII16SRAM2,
9 = ASCII16SRAM8, 10 = R-Type. These names describe hardware, not verified game
compatibility. Game Master 2, its second-cartridge-slot use, Koei-specific wiring,
32 KiB SRAM, ESE-RAM, SCC+, disk/tape and other specialized mappers remain outside
this single-cartridge implementation.

## Hardware contract and sources

The register decoder and executable fixtures were independently authored from
hardware descriptions, without copying emulator implementation code or game ROMs:

- Sean Young's [MegaROM hardware investigation](https://bifi.msxnet.org/msxnet/tech/megaroms)
  ([archive mirror](https://ftpmirror2.infania.net/sites/bifi.msxnet.org/msxnet/tech/megaroms))
  describes measured ASCII register ranges, Xanadu/Royal Blood SRAM selects,
  Hydlide II's 2 KiB repetition and write protection, and R-Type's two ROM chips.
- The [MSX MegaROM mapper reference](https://www.msx.org/wiki/MegaROM_Mappers)
  describes SRAM select pins and explicitly warns that wiring depends on hardware.
  Its [alternate URL](https://www.msx.org/wiki/ROM_mappers) covers the same families.
- The [ASCII16 SRAM usage experiment](https://www.msx.org/forum/msx-talk/openmsx/how-to-emulate-asc16-with-write-support)
  reports bit 4 selection for the 8 KiB variant. SRAM capacity alone does not
  determine a mapper's register width or select bit.
- The [openMSX save FAQ](https://openmsx.org/manual/faq)
  distinguishes a cartridge's embedded SRAM from external Game Master 2 saves.

| Mapper | ROM image limits | SRAM select | SRAM address mirror |
| --- | --- | --- | --- |
| ASCII8SRAM2 | Power of two, 8 KiB–1 MiB | First bit above physical ROM bank bits: `ROM length / 8192` | CPU address AND 07FFh |
| ASCII8SRAM8 | Power of two, 8 KiB–1 MiB | Same size-dependent line: 20h for 256 KiB, 80h for 1 MiB | CPU address AND 1FFFh |
| ASCII16SRAM2 | Power of two, 16–256 KiB | Bit 4 (10h); upper bits ignored | CPU address AND 07FFh |
| ASCII16SRAM8 | Power of two, 16–256 KiB | Bit 4 (10h); upper bits ignored | CPU address AND 1FFFh |
| R-Type | Exactly 384 KiB, 256 KiB chip then 128 KiB chip | No SRAM | See chip decode below |

ASCII8 choices implement the conventional size-dependent select wiring. This
covers the documented 256 KiB/20h and 1 MiB/80h layouts without using game names
as switches. Other fixed select lines, separate SRAM page bits, bank registers
with reversed order, outer CPU-page mirrors or Koei-specific behavior cannot be
inferred from an SRAM capacity label and are not claimed. Supply an unpadded ROM
of its physical size and choose the documented mapper. Unsupported partial or
oversized images fail explicitly. Smaller ROM address lines wrap by physical
size; SRAM selection is decoded before ROM wrapping. SRAM low bank bits are
ignored, rather than selecting arbitrary writable ROM banks.

ASCII8 registers decode every address in 6000h–67FFh, 6800h–6FFFh,
7000h–77FFh and 7800h–7FFFh, controlling the four 8 KiB windows at
4000h–BFFFh. ASCII16 decodes 6000h–67FFh and 7000h–77FFh, controlling
two 16 KiB windows. All reset bank registers are zero. SRAM is readable in any
selected window but writable only at 8000h–BFFFh, and only while that window's
register selects SRAM. Page 1 stays write protected; mapper register writes still
work there while SRAM is readable. SRAM mirrors across each window and shares
physical bytes across SRAM-selected windows. ROM stays protected. These choices
expose no cartridge data outside 4000h–BFFFh. All accesses, including registers,
require cartridge slot 1 selected in that CPU page.

R-Type fixes 4000h–7FFFh to bank 0Fh and resets 8000h–BFFFh to bank 0.
Every write at 7000h–7FFFh (including 7800h) changes only the second window.
Bit 4 selects the chip: zero uses bits 0–3 (banks 00h–0Fh); one uses bits 0–2
(banks 10h–17h), ignoring bit 3. Bits 5–7 are unused. 6000h–6FFFh does not switch
the fixed window. There is no modulo-384-KiB shortcut. Original hardware has
identical header/code in banks 0Fh/17h; the loader uses bank 0Fh as the fixed
window and does not rewrite or demand equality of those physical ROM contents.
AB validation checks the actual initial CPU windows, not file offset zero.
The R-Type size exception does not relax other mappers' power-of-two rules.

## Detection and lifecycle

Auto never selects a new mapper from size, header or SRAM-looking opcodes alone.
It still estimates existing ROM-only families. Known immediate ASCII register
writes that assert a possible SRAM select line suppress an otherwise confident
ROM-only guess and explain the manual choices. Incomplete static analysis cannot
prove the absence of SRAM; follow the ROM author's hardware documentation.
R-Type requires manual selection, including when the image is exactly 384 KiB.

Validation borrows ROM bytes and completes size/header checks before reserving
SRAM with `try_reserve_exact`. The owned upload then commits ROM, mapper and SRAM
without a second ROM copy. A failed validation, SRAM allocation or display/model
reservation leaves the old CPU, model, firmware, bank registers and unsaved SRAM
intact. SRAM starts erased (FFh) for every newly installed core cartridge.

Reset resets bank registers but retains SRAM and its select wiring without
allocating. Local firmware imports also reset while retaining that cartridge's
SRAM. Eject, replacement, model change, Use Weave BASIC and explicit bundled
firmware selections release the old cartridge's memory. Automatic cartridge
C-BIOS boot installs the incoming cartridge's own SRAM; no other cartridge inherits
it. The browser saves outgoing SRAM before these transitions and restores by
identity when its ROM is reopened. Firmware and model choices do not change the
save identity.

## Browser saving and recovery

Choose a mapper, then **Open ROM**. For SRAM choices the browser computes a local
SHA-256 of the original bytes before committing the import. After core validation,
it restores the matching local save synchronously before any CPU frame executes.
The key is `msx-rust:sram:v1:<ROM SHA-256>:<mapper ID>`. ROM images are never stored
or uploaded. Renaming a file preserves its save; changing bytes or mapper gives
another key. Different browser profiles/origins have separate storage.

Only bounded 2/8 KiB SRAM snapshots are copied. About once a second, on tab hiding,
on pagehide, and before cartridge changes, changed data is written to
`localStorage`. Export a backup for durability across private sessions, browser
data clearing, storage eviction or an abrupt process/device crash. At most the
recent interval of writes can be lost on a crash. Storage is not a save-state:
CPU/RAM/video state and BASIC programs are not persisted. Games must implement
saving to this SRAM hardware themselves.

Advanced options show the save status and **Export SRAM backup** /
**Import and replace SRAM**. JSON backups contain version 1, ROM hash, mapper ID,
exact SRAM size, canonical base64 data and an FNV-1a checksum. Import validates
all fields, size and checksum before replacing any bytes; malformed, wrong-game,
wrong-mapper and corrupt data leave current SRAM intact. FNV is an accidental
corruption check, not authentication. The raw Rust restore API checks exact
length; container identity/version validation belongs to the browser caller.
Wasm exposes `msx_sram` / `msx_sram_len` views. The browser copies validated
bytes into the reserved view synchronously between CPU frames, avoiding another
fallible Wasm allocation after cartridge commit. Other ABI callers can use a
staged `msx_restore_sram(length)` upload with Rust size validation. Reacquire
views after Wasm mutations.

Reset after an explicit save import if the game reads SRAM only at startup.
Exporting never erases data. There is no delete button; browser site-data clearing
removes local saves (export first). Import explicitly replaces that cartridge's
current SRAM and stored record. It is guarded against stale asynchronous reads
when the cartridge or session changes.

Quota/permission failures are nonfatal and appear separately from execution
status. SRAM remains usable in memory and export is available. A corrupt stored
record is preserved; automatic writes pause until explicit valid import, so new
blank SRAM cannot silently destroy the only existing record. Another tab's changed
record also blocks overwrites; export the current tab's data and explicitly import
the intended backup to resolve it. A reopened identical cartridge in the same tab
retains its in-memory snapshot even if persistence failed. Saves are not account
synced or sent to a server.

## Executable evidence

`firmware/sram_fixture.rs` generates original MIT-licensed probes (`mapper-6.rom`
through `mapper-10.rom` in dist). SRAM probes boot with C-BIOS, execute code from
ROM bank 7, copy a routine into RAM, select SRAM, read it through a mirror and show
its value as VDP backdrop. Space increments once per press. The R-Type probe has
its AB header in fixed bank 0Fh, selects bank 17h through 7FFFh, executes it and
shows yellow. No commercial cartridge bytes or other emulator code are included.

`tests/sram.rs` inspects actual bus reads/writes, RAM markers, VDP registers and
SRAM for all variants on MSX1/MSX2. It covers register ranges/holes, select bits,
physical limits, mirrors, write protection, hidden slots, reset retention,
replacement/eject/firmware/model isolation, restore size validation, failed
imports and conservative Auto. `tests/allocation.rs` injects real allocation
failures before commit and proves Reset does not allocate or erase SRAM.
`tests/sram.spec.js` imports the generated probes using the real picker and
published-format Wasm at both `/` and `/msx-rust/`. It verifies both models,
visible execution, local SHA-256 identity, export/import, a fresh page's restore,
wrong/corrupt/oversized saves, failed ROM/memory transfers, ROM/mapper isolation,
storage failures and cross-tab conflict preservation. Existing BASIC, C-BIOS,
demos, keyboard/video and classic SCC tests remain required regressions.

Required post-merge verification remains pending: after Pages publication,
record deployed commit, successful Actions run and the live
<https://takahirox.github.io/msx-rust/> URLs. Run an SRAM and R-Type probe through
the published picker/Wasm, recover SRAM after a fresh page, and confirm MSX2
BASIC and curated demos. File a separate Issue for a deployed-site failure.
Local checks do not establish publication.

## Local validation record

Validated on 2026-10-21, macOS arm64, pinned Rust 1.99.0, Node.js 24.12.0,
Playwright 1.64.1 / Chromium:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed: 103 tests, including 7 SRAM/layout tests and 4 allocation tests |
| `./scripts/build.sh` | Passed using the existing documented macOS linker wrapper |
| `node scripts/check-dist.mjs` | Passed: inventory, original probe lengths/headers, SRAM exports, local module, pinned existing ROMs/notices and relative links |
| `npm ci`; `npx playwright install chromium` | Passed |
| `npm test -- --workers=2` | Passed: all 74 tests in 4.8 minutes; 14 new SRAM/layout tests at both URL roots |
| JavaScript syntax, relative Markdown file links, `git diff --check` | Passed |

The unwrapped build first hit the repository's known macOS `libLLVM.dylib`
loader error. The ignored [linker wrapper](deployment.md#pre-merge-validation)
selects the same pinned linker and library path; no toolchain or CI change was
needed. The initial full `npm test` ran 72 tests: 70 passed and two C-BIOS tests
rejected the newly added local `saves.js` request in their static-asset allowlist.
Their emulation assertions passed. Updating that allowlist and rebuilding produced
the complete passing 74-test run, including added raw restore and stale-save
lifecycle coverage. No checks remain failing or skipped.

Inspected `test-results/sram-6-pages.png`: the actual emulated saved backdrop,
manual mapper choice, backup controls and successful SRAM import status are
visible. Browser tests also assert exact pixels and restored bytes; Rust tests
inspect banked execution RAM markers and VDP state. Screenshots and detailed
runner logs are ignored local test artifacts and can be regenerated with the
commands above. No proprietary game ROM or manual gameplay was used.

Implementation and this local validation are complete. This node does not push,
open a PR, comment, merge or deploy. PR/AI review and Issue #27's post-merge live
Pages verification remain for the subsequent workflow; the local results do not
claim that those steps happened.

## Independent FM-PAC accessory storage

[External FM-PAC](fm-pac.md#shared-accessory-saves) adds a separate firmware/profile
identity and namespace with exactly 8190 usable bytes. Its save is shared across
games; cartridge replacement never changes it. The same conflict, corruption,
storage-error and explicit backup/recovery rules apply independently. Cartridge
save validation still accepts only 2048/8192-byte payloads for its active mapper.
