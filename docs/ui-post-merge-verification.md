# Browser UI post-merge verification

Issue [#7](https://github.com/takahirox/msx-rust/issues/7) required verification
of the published UI after merge. This checkpoint record completes that check;
the pending sections in earlier validation records describe their pre-merge state.

## Merge and deployment

- Issue #7 was read immediately before merging and confirmed open.
- [PR #9](https://github.com/takahirox/msx-rust/pull/9) was mergeable, with a
  successful browser-app CI build and no blocking findings in the upstream review.
- GitHub merged with the expected head
  `c9e2df1943d7c09bebfe3fa50fc080e595cc7df3`, using a merge commit:
  `9e79661a899823317a63c6a4faa94e148ebff6d3`.
  The merge commit has two parents, including that exact PR head.
- Pages publishing configuration reports `build_type: workflow`.
- The merged commit's [Pages workflow run](https://github.com/takahirox/msx-rust/actions/runs/38935071383)
  completed successfully, including both build and deployment jobs.
- Deployment `7124605863` reports success for that merge commit; its
  [deployment job](https://github.com/takahirox/msx-rust/actions/runs/38935071383/job/117088782855)
  reports the published URL as <https://takahirox.github.io/msx-rust/>.
  GitHub recorded deployment success at `2026-10-16T16:51:40Z`.

## Published-site check

An isolated headless Chromium session checked the actual HTTPS site at
`2026-10-16T16:52:24.618Z`, using Playwright and a 1280 × 900 viewport.
The existing Chrome DevTools connection was unavailable, so verification used
Playwright's independently launched Chromium instead.

- The document, relative `app.js`, `style.css`, and `msx_wasm.wasm` requests all
  returned HTTP 200. No failed requests, HTTP errors, or page exceptions occurred.
- The UI reached `Running · Weave BASIC`, and the transcript contained `READY`.
- Clicking **BASIC example** produced `1`, `2`, `3`, `4`, `5`, then `READY` in
  the transcript. The native 256 × 192 Canvas rendered 1,976 white pixels.
- Published HTML, JavaScript, and CSS matched the reviewed local source files
  byte for byte using SHA-256:

| Published file | SHA-256 |
| --- | --- |
| `index.html` | `de0d71ec7ba91cef01605d666b5921fb463e910dc51b3804e25316a50746c2bd` |
| `app.js` | `1fb86a561b9a5493faf2e35971ced8a17e1738e9be4e47264c3668d8c6446a34` |
| `style.css` | `8c4b85035ef01d22fcb2d9efa1a5d734c5b019601df235853e093172c538bf77` |

Required post-merge publication verification passed. Optional human visual and
physical-device review remains optional. This production smoke check supplements
the full pre-merge validation; it does not claim a production rerun of every test.
