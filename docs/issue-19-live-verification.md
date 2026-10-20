# Issue #19 merge and live Pages verification

Required post-merge verification **passed** on 2026-10-20 at
08:52 UTC against <https://takahirox.github.io/msx-rust/>.
This record completes the pending verification for Issue #19 in
[the MSX2 documentation](msx2.md#required-post-merge-verification--pending).

## Merge and deployment

- Source: [Issue #19](https://github.com/takahirox/msx-rust/issues/19).
- Run: `a1103e0bd1604c61abc96d11864135d5`.
- [PR #20](https://github.com/takahirox/msx-rust/pull/20) merged at
  `2026-10-20T08:45:59Z` with a merge commit.
- Reviewed and merged PR head: `df0b1db3bbcbc0a6a5ec4921545648edddfc4649`.
- Merge commit: `cc80cc69fe62b854065988f15ca6e90b6e13bea4`.
- Merge parents: `c75428533433c67e0a10a41a77f6c584f1b9f956` and
  `df0b1db3bbcbc0a6a5ec4921545648edddfc4649`.
- [Build and Pages deployment run 39395770139](https://github.com/takahirox/msx-rust/actions/runs/39395770139)
  succeeded for that merge commit.

Issue #19 was read immediately before merging and confirmed OPEN. The merge
request specified the reviewed head SHA and the `merge` method. GitHub reported
the PR mergeable with its build check successful; no policy bypass was used.
GitHub subsequently reported PR #20 MERGED and Issue #19 CLOSED.

The merge-triggered workflow passed formatting, Clippy, all 82 Rust tests,
the Wasm build, deployment inventory validation, and all 43 browser tests
covering local root and project-prefix hosting, before deploying Pages.

## Actual published application

Headless Chromium 156.0.8078.4 / Playwright 1.64.1 ran two additional browser
tests against the live HTTPS project URL. They adapted the existing
[boot-policy tests](../tests/boot-policy.spec.js) to the published URL and used
the Canvas digit recognition from [browser tests](../tests/browser.spec.js).
The original, redistributable cartridge fixtures and local firmware inputs
were downloaded from the deployed site. No proprietary ROM was used.

Both tests passed with no retries, failures or skips in 31.2 seconds:

| Observed behavior | MSX1 | MSX2 |
| --- | --- | --- |
| Automatic startup into Weave BASIC | Passed | Passed |
| Original BASIC loop prints 1–5, recognized from Canvas pixels | Passed | Passed |
| INPUT and stored-program execution | Passed | Passed |
| Invalid/empty/ambiguous imports preserve the running BASIC program | Passed | Passed |
| SCREEN 0–3 visible output and drawing | Passed | Passed |
| SCREEN 5–8 dimensions, colors and drawing pixels | N/A | Passed |
| Open ROM file picker boots the independent BIOS-service cartridge through model-specific C-BIOS | Passed | Passed |
| Cartridge Canvas pattern and Space-key input | Passed | Passed |
| Reset boots the retained cartridge | Passed | Passed |
| Eject returns to BASIC; `PRINT 91` executes | Passed | Passed |
| Explicit C-BIOS and local firmware overrides survive load/Eject | Passed | Passed |
| Use Weave BASIC clears the override; `PRINT 86` executes | Passed | Passed |
| Page errors, failed requests and HTTP errors | None | None |

All **18** deployed files returned HTTP 200 and matched SHA-256 hashes of the
`github-pages` artifact downloaded from deployment run 39395770139. This includes
HTML, JavaScript, CSS, Wasm, the seven independent ROM fixtures, three official
C-BIOS ROMs, and all four license/provenance files.

Published Wasm: 357,935 bytes, SHA-256
`e5449def7c24b9041ad2f0b689ff581423b8f3dfd7ee878c1b0207c3f199de4a`.
Published `app.js`: 18,008 bytes, SHA-256
`60c674d83e2dbe8f6d74997f24a92da326ec237700e07c4ee70d90a0125c59e7`.

No post-merge problem was observed, so no follow-up Issue was needed. These
checks establish the specified deployed workflows; they do not claim full
Microsoft MSX BASIC compatibility or compatibility with commercial cartridges.
