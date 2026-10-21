# Cartridge SRAM and R-Type live verification

Issue [#27](https://github.com/takahirox/msx-rust/issues/27)'s required
post-merge verification passed on 2026-10-21 (UTC), using Playwright 1.64.1 and
Chromium against <https://takahirox.github.io/msx-rust/>.

## Merge and publication

- PR [#28](https://github.com/takahirox/msx-rust/pull/28) merged with a merge
  commit: `6e010748c780dce56c74b1bfce4a678035562342`.
- The source Issue was read and confirmed open immediately before merging.
  GitHub accepted the expected head SHA
  `e21c34f1d0aec47689b8763ae1702823bd60ef88`; the merge's parents are
  `dcebd6bb440251cc6d173ffda2ba214acae2b525` and that reviewed head.
  Required checks and reviews were not bypassed.
- [Actions run 39536926614](https://github.com/takahirox/msx-rust/actions/runs/39536926614)
  succeeded for the merge commit. Formatting, Clippy, all 103 Rust tests, the
  Wasm build, distribution checks and all 74 browser tests passed. The browser
  suite covered both `/` and `/msx-rust/` and took 8.3 minutes on CI.
- GitHub Pages deployment `7216776555` reported success at
  `2026-10-21T03:18:57Z`, with the live URL above.
- Eleven live assets returned HTTP 200 and matched this run's `github-pages`
  artifact byte for byte. HTML, CSS and both JavaScript modules also matched
  the reviewed source. The published Wasm is 367,653 bytes, SHA-256
  `ef1f8e7932c255ad7b303facaf3d85dbe05a8dd0f97bfd444d03790b60bcd65b`.
  [Asset URLs, sizes and hashes](cartridge-sram-evidence/live-assets.json)
  record the live responses checked at `2026-10-21T03:19:09Z`.

## Live results

Four targeted browser tests passed in 19.1 seconds, with no retries or skipped
tests. These used the published page and Wasm, with the original generated
fixtures downloaded locally and selected through the real ROM picker.

| Check | Observed result |
| --- | --- |
| Default MSX2 BASIC | A fresh page, new tab and reload started MSX2 Weave BASIC. Canvas output showed 1–5; SCREEN 8 drew the expected red pixel. Switching to MSX1, Reset and returning to MSX2 worked. |
| ASCII8SRAM2 | `mapper-6.rom` executed banked Z80 code through cartridge C-BIOS. SRAM started at 4 with backdrop RGB `(36,36,255)`; Space advanced it to 5 with RGB `(72,109,255)`. Reset retained it. A fresh page reopened the ROM, reported restoration, and recovered both the byte and visible saved backdrop. |
| Save integrity and isolation | Invalid ROM imports and invalid/corrupt/wrong-identity backups preserved the active SRAM. Changed ROM bytes and mapper variants received separate saves; renaming the original ROM restored its existing save. A corrupt local record remained intact until explicitly replaced with a valid backup. |
| Other SRAM choices | ASCII8SRAM8, ASCII16SRAM2 and ASCII16SRAM8 also executed and advanced their SRAM value through the real picker on MSX1. |
| R-Type | The exact 393,216-byte `mapper-10.rom` executed its second ROM chip and showed yellow: RGB `(212,193,84)` on MSX1 and `(218,218,36)` on MSX2. Reset retained correct behavior. A 512 KiB image was rejected without disturbing execution; Auto requested manual R-Type selection. |
| Curated demos | Both plustaker and YOKO drew their game scenes and player sprites, responded to movement/fire, and reset. The MSX2 diagnostic switched video modes and produced expected pixels. Eject returned to BASIC, which executed `PRINT 73`. Manifest ROM and notice URLs loaded successfully. |

The tests observed no page errors or failed asset responses. The SRAM and demo
flows made only GET requests within the site's `/msx-rust/` prefix. ROM images
were not uploaded or persisted. No deployed-site failure was found, so no
follow-up defect Issue was needed. These are synthetic fixture and curated demo
results, not a claim of universal game compatibility.

## Method and retained evidence

The four tests were selected from the existing
[`browser.spec.js`](../tests/browser.spec.js),
[`demos.spec.js`](../tests/demos.spec.js) and
[`sram.spec.js`](../tests/sram.spec.js):

- `fresh loads start MSX2 BASIC before the first frame`
- `curated games and MSX2 diagnostic execute with input, switching, reset and eject`
- `ASCII8SRAM2 real Z80 SRAM, reload, identity isolation and invalid saves`
- `R-Type chip layout and MSX1 SRAM manual choices`

Temporary copies replaced the two local site URLs with the live URL, used the
downloaded live fixtures instead of local `dist/`, and ran with a standalone
Playwright configuration without local web servers. Assertions and the existing
Wasm instrumentation were retained: deterministic frame stepping executes the
actual published CPU/VDP; it does not substitute an emulator or Wasm response.
Two screenshot captures were added for BASIC and R-Type. Existing captures
recorded the restored SRAM backdrop and running demos.

Inspected screenshots of [restored SRAM](cartridge-sram-evidence/sram-restored.png)
and [R-Type](cartridge-sram-evidence/rtype.png) show the expected blue and yellow
execution output and selected mapper. Also retained:
[MSX2 BASIC](cartridge-sram-evidence/msx2-basic.png) and
[plustaker](cartridge-sram-evidence/plustaker.png).

This record and its evidence are saved in the GitWeave merge-node checkpoint;
they were created after the production merge and are not part of the deployed
commit identified above.
