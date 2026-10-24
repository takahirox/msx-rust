# Optional original FM-PAC

Issue [#75](https://github.com/takahirox/msx-rust/issues/75) provides one explicit
external profile on MSX1 and MSX2. Supply a separate **64 KiB firmware image**
under **ROMs & advanced system options → External FM-PAC**. Installation is
optional; a fresh machine has no FM-PAC. Image length selects the supported
layout, not firmware authenticity or compatibility. No header, identification
string, BIOS API or firmware patch is fabricated by the emulator.

## Slots and registers

This selected virtual configuration expands primary **slot 2** only while the
accessory is installed. Disk stays in **2-0/page 1**, FM-PAC occupies **2-1/page 1**,
and 2-2/2-3 are empty. Slot 1 still independently holds a game or SCC-I. Each
page uses its own two-bit secondary selector. Whenever primary 2 occupies page 3,
FFFFh reads the inverted selector and writes its latch, before device decoding,
regardless of the page-3 subslot. Reset clears slot-2 selectors to zero. Removal
restores unexpanded slot 2. MSX2 slot 3 remains independently expanded; MSX1
slot 3 remains fixed RAM, including its ordinary FFFFh byte.

| Address | Original profile behavior |
| --- | --- |
| 4000h–7FFFh | Supplied read-only firmware in four 16 KiB banks; exact bytes under overlays |
| 7FF7h | Read/write bank, retains low two bits |
| 7FF6h | Read/write control, retains bits 0 and 4; reset zero |
| 5FFEh / 5FFFh | Writes 4Dh / 69h unlock SRAM; either wrong latch relocks |
| 4000h–5FFDh | 8190 usable battery bytes when unlocked; initialized FFh on new installation |
| 5FFEh / 5FFFh reads | 4Dh / 69h when unlocked; underlying banked ROM otherwise |
| 6000h–7FFFh reads while unlocked | FFh except control/bank at 7FF6h/7FF7h |
| 7FF4h / 7FF5h writes | YM2413 address/data, independent of SRAM mode and I/O enable |

Control bit 4 clears both unlock latches and prevents their writes until cleared.
Partial/reverse-order unlocks keep their individual latch values; SRAM is visible
only for the complete pair. Protected writes cannot modify ROM or SRAM. Bank and
control overlays remain readable in SRAM mode; memory sound registers are
write-only overlays and read ROM when locked, FFh when unlocked.

Control bit 0 connects external OUT 7Ch/7Dh. Only the CPU port's low byte decodes;
port reads return FFh. Disabling I/O or hiding the device never stops programmed
sound or resets phase. The independent internal MSX-MUSIC chip, if installed on
MSX2, receives those writes as before. Enabled recipients each receive a write;
there is no host discovery, auto-enable or duplicate-sound suppression. Firmware
must scan slots and decide what to enable. Software assuming an unexpanded slot 2
or only scanning slot 1 may need different slot-scanning logic.

## Lifecycle and interfaces

`Machine::install_fm_pac(Vec<u8>)` validates size and reserves SRAM before changing
the session. `has_fm_pac`, `remove_fm_pac`, `fm_pac_sram` and
`restore_fm_pac_sram` provide independent presence/removal/save access. Restore
requires exactly 8190 bytes and commits atomically. Installation/replacement starts
with new FFh SRAM; a host may restore its previously retained save afterward.
Removal releases firmware/SRAM. Reset retains the installation and battery bytes,
but resets bank/control/unlock latches and the entire sound chip.

Installation/removal restarts selected firmware while preserving the game/SRAM or
SCC-I RAM, disk interface, mounted media and changes, and cassette position.
Firmware, game and model transitions retain FM-PAC firmware/SRAM and reset its
controls/chip consistently. Existing transition rules for other devices remain.
Failed size/profile checks, file reads, reservations, canceled/stale imports leave
the running session intact. Browser imports use the common generation token and
staged upload; SHA-256 identity finishes before staging or committing.

Wasm exports `msx_install_fm_pac(length, profile)` (profile **1** only),
`msx_fm_pac_profile()` (0/1), `msx_remove_fm_pac()`, `msx_fm_pac_sram()`,
`msx_fm_pac_sram_len()` and `msx_restore_fm_pac_sram(length)`. Firmware/save
transfers reuse `msx_upload`, cancellation and error strings. The mono f32
44.1 kHz audio ABI is unchanged. Memory views must be reacquired after allocations.

Synthesis reuses the separately noticed MIT emu2413 adaptation. Both chips advance
on CPU clocks even when hidden/I/O-disabled, share the existing rational sample
boundary, sum with PSG/SCC and clamp the combined FM mix to [-1,1]. With no FM-PAC,
existing PCM/counts are unchanged. Pause freezes emulation; browser mute gates
playback only. The existing instruction-level timing, held sample resampling,
12-clock address / 84-clock data recovery guidance, fast-write policy and analog
limitations still apply; see [MSX-MUSIC synthesis and notices](msx-music.md).
MSX-AUDIO, standalone PAC variants, generalized expansion, exact silicon fast-write
behavior, vendor FM-BIOS execution and commercial playability remain unestablished.

## Shared accessory saves

FM-PAC SRAM belongs to the accessory and is **shared across games** using the same
firmware/profile. Its separate key is
`msx-rust:fm-pac:v1:<firmware SHA-256>:original-fm-pac`; the filename and active game
have no effect. Only the 8190-byte payload is persisted, never firmware bytes or
unlock latches. Reset, model/firmware/game replacement keep that active save.
After reload, explicitly reinstall identical firmware to recover its local save.
Other firmware bytes get a separate identity. Removal saves outgoing bytes first;
export a backup before removal if browser storage is unavailable.

**Export FM-PAC SRAM backup** / **Import and replace FM-PAC SRAM** use version-1
JSON with firmware digest in `rom`, profile `original-fm-pac` in `mapper`, size
8190, canonical base64 data and FNV-1a checksum. The container keeps the established
field vocabulary, with a separate namespace and strict accessory decoder. Every
field and payload/checksum validates before mutation. Cartridge saves still accept
only their original 2048/8192 sizes; an accessory save cannot pass that decoder.

Automatic writes compare the previously read record to detect another tab. Corrupt
or conflicting records are preserved and further writes blocked; live emulation
and backup export continue. Explicit valid import authorizes replacing that record.
Storage errors keep bytes in memory; reopening the same firmware retains those
unsaved bytes. Private browsing/cleared data can remove local saves. Safeguards and
UI behavior are shared with [cartridge saves](cartridge-sram.md#browser-saving-and-recovery).

## Independently authored diagnostics and validation

`fm-pac-diagnostic-firmware.rom` is original MIT firmware with the deliberate
**WVFPTEST** marker, not a vendor identifier. Four executable routines at 4100h
return **51h,52h,53h,54h** in banks 0–3. Its AB init pointer is zero.
`fm-pac-test-cartridge.rom` runs in game slot 1/page 2 under matching C-BIOS on both
models (or the original diagnostic loader on MSX2). It scans all four subslots of
primary 2, tests the FFFFh latch, calls all four banks, checks partial unlock and
unlock readback, and reads/writes/verifies every usable SRAM byte unchanged.

It programs a custom sine through memory while I/O is disabled. Enabled port
writes change F-number 273 to 290; disabled writes try 324. Then it hides the
accessory and halts with sound continuing at F-number 290 / block 4. Exact success
RAM at **C100h–C109h** is **01 FB 51 52 53 54 FF 4D 69 A5**. The backdrop is green;
missing/incorrect behavior fails red. Native and browser tests independently use
`44100 * 72 * 2^19 / (290 * 16 * 3579545)` samples per period, allow two output
samples of crossing quantization, and require amplitude 0.015–0.016. Nonzero sound
alone cannot pass. Diagnostic sound writes respect the documented recovery clocks.

`tests/fixtures/fm-pac-disk.py` independently adds page-0 Z80 expansion/enable setup
to the original Philips system fixture. Combined with the existing original
FM/PSG disk ROM and SCC-I sector, native tests on both models and compiled-Wasm
browser tests exercise exact C030h–C033h results **44 60 6D A5**, sector loading,
SCC-I RAM execution and exact disk export (only sector 8 becomes 512 bytes of 6Ch).
No proprietary firmware or commercial title is used.

Native coverage includes all bank bytes, memory overlays, unlock/protection/reset,
slot boundaries, latch precedence, hidden isolation/disk separation, transactional
allocation failure and save size rejection, lifecycle retention, independent
chips/broadcast, precise PCM equality and partition invariance/sample counts.
Browser tests at `/` and `/msx-rust/` cover real picker/Wasm imports, scheduled PCM,
mute/resume/pause/reset/removal, exact save/reload/game replacement, corrupt and
conflicting records, storage failure and export/import recovery, stale/canceled
file reads, bounded real-Wasm allocation failure, and combined disk/SCC-I/PSG/FM.

`python3 tests/reference/missing-fm-pac.py` substitutes baseline
`fb5e220db81a609fb8bbe0eab1cdd7696f589b9e`'s complete Bus implementation in a
temporary copy, retaining setup APIs. Observed baseline failures:

- Bank/control expectation: **FFh received, 00h expected**.
- Z80 success bytes: **F0 00 00 00 00 00 00 00 00 00** received instead of
  **01 FB 51 52 53 54 FF 4D 69 A5**.

The corrected tests pass with current decoding. This is a sensitivity check,
not execution of the new API against an unchanged baseline binary.

Mandatory README checks and source-Issue/full-diff review are recorded below. Optional comparisons against a running pinned reference or physical
hardware, lawfully supplied vendor firmware and commercial gameplay were not
performed. No required post-merge verification.

## Behavioral references and provenance

- [MSX datapack MSX-MUSIC hardware chapter](https://map.grauw.nl/resources/datapack/Vol2-3.1MSX-MUSICHardware.pdf), 3.1.2–3.1.4: memory/I/O, identification, enable interaction and write spacing.
- [Pinned openMSX MSXFmPac.cc](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/src/sound/MSXFmPac.cc): behavioral corroboration of original bank/SRAM/control decoding and memory independence from I/O enable. No device implementation code copied.
- [PAC SRAM investigation](https://gigamix.hatenablog.com/entry/devmsx/pac-sram-access) and [pinned save guidance](https://github.com/openMSX/openMSX/blob/26e1e4945df406e250a05122889f88d40edd828b/doc/manual/faq.html): usable bytes exclude unlock locations; accessory saves are shared.

Device decoding, Z80 fixtures, synthetic data and test expectations were authored
independently under MIT. Existing synthesis attribution, tables and full upstream
notice are retained unchanged; [asset inventory](../firmware/ROM-LICENSE.txt) and
[deployment inventory](deployment.md) include the two new original ROM assets.

## Local execution record (2026-10-24)

| Check | Observed result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 321 passed; no failures or ignored tests |
| `./scripts/build.sh` | Passed with pinned Rust 1.99.0 and the local linker setup below |
| `node scripts/check-dist.mjs` | Passed; 44 deployment files, required exports, original fixtures, unchanged C-BIOS and synthesis notices |
| `npm test` | 218 passed in 15.1 minutes, including 12 FM-PAC cases across both site roots |
| Final `npx playwright test tests/fm-pac.spec.js --reporter=line` | 12 passed in 25.4 seconds after the final build; strengthened actual scheduled-buffer pitch/gain and separate firmware-identity recovery assertions passed |
| `python3 tests/reference/missing-fm-pac.py` | Both expected baseline assertion failures reproduced; current corrected tests pass |
| Slot/enable fixture regeneration | Byte-for-byte match in a temporary directory |
| Accessory/cartridge save decoder isolation | Exact round trips and reciprocal size/profile rejection passed |
| Local Markdown links; `git diff --check` and staged whitespace check | Passed |

README dependency setup (`npm ci`, Chromium installation and pinned-toolchain
`PATH`) was used. The first Wasm link failed because this host's pinned `rust-lld`
could not locate `libLLVM.dylib`. As already documented for internal MSX-MUSIC,
an ignored local wrapper under `target/` set `DYLD_LIBRARY_PATH` to the same
pinned sysroot's `lib` and invoked that toolchain's `rust-lld`. The subsequent
builds passed; no alternate compiler, dependency, project linker configuration or
host-wide change was introduced. Initial diagnostic/test-harness issues were
corrected before the passing full-suite runs. Final browser assertions were rerun
against the refreshed distribution. No mandatory check remains failed, skipped or
unperformed.

Source-Issue/full-diff self-review covered slot/latch precedence, register and
SRAM overlays, independent I/O/chips, transactional lifecycle, shared save
identity/recovery, native and compiled-Wasm diagnostics, mixing/PCM, and notices.
It clarified additional *game* cartridge limits and corrected the deployment
inventory (including the existing horizontal-IRQ fixture). No blocking finding
remains. This node commits locally only: no push, PR, review comment, merge or
publication. Downstream PR review remains part of the repository workflow.
Optional reference-runtime/physical-hardware comparisons, vendor FM-BIOS execution
and commercial gameplay were unperformed; no such compatibility is claimed.
Required post-merge verification: none.
