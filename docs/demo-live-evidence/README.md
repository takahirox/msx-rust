# Issue #21 live deployment verification

Verified on 2026-10-20 against the published
[GitHub Pages site](https://takahirox.github.io/msx-rust/).
This completes Issue #21's required post-merge verification and supersedes its
pre-merge pending status in the PR and deployment documentation.

- PR: [#22](https://github.com/takahirox/msx-rust/pull/22).
- Merge and deployed commit: `e8ee30ad09acd5c376cc6f1fc313a850cd83313c`.
- [Actions run 39406681499](https://github.com/takahirox/msx-rust/actions/runs/39406681499):
  build, mandatory checks, browser suite and Pages deployment succeeded.
- GitHub Pages deployment `7197233280` reported success at
  `2026-10-20T10:21:34Z`, with the commit and site URL above.
- Pages publishing source is GitHub Actions (`build_type: workflow`).

The curated-execution test from `tests/demos.spec.js` was adapted to use only
the live HTTPS site, omitting the local-file import check. Playwright 1.64.1 /
Chromium passed the test in 13.0 seconds. No local server, substituted ROM,
mocked response or locally built Wasm was used. As in the existing suite, the
real deployed Wasm CPU/VDP was stepped deterministically for game movement
assertions; normal animation resumed for the diagnostic and BASIC checks.

Verified behavior:

- Selected **plustaker** and **YOKO** through Demo ROMs. Each selected MSX1,
  Plain mapper and automatic matching C-BIOS. Observed game HUD/player pixels,
  upward player movement from ArrowUp, firing input, and Reset restoring the
  player position. YOKO also responded to Space to start gameplay.
- Switched from a game to the MSX2 diagnostic. Observed SCREEN 7 pixels and
  512-wide canvas, then held **8** and observed SCREEN 8 pixels and a 256-wide
  canvas. Reset restored SCREEN 7.
- Ejected game and diagnostic cartridges to same-model Weave BASIC. Executed
  `PRINT 73` after MSX2 Eject and observed `73` followed by `READY`. Also
  verified **Use Weave BASIC** after launching another MSX1 game.
- Fetched every manifest ROM, license and notice at its deployed URL.
  All three ROM sizes and SHA-256 hashes matched the tracked manifest; all four
  pinned notice hashes matched. The project LICENSE and ROM-LICENSE.txt also
  loaded successfully.
- Observed no page errors or HTTP errors. Browser requests stayed within the
  site's static asset prefix, used GET and carried no upload body. No storage
  writes occurred. The 320px viewport had no horizontal overflow.

Inspected screenshots from the published site:
[plustaker](plustaker-pages.png), [YOKO](yoko-pages.png),
[MSX2 SCREEN 8](msx2-pages.png), and [mobile layout](mobile-pages.png).
These checks cover the required execution/control paths, rather than full game
completion or audio quality. No live-site defects were found requiring a
follow-up Issue.
