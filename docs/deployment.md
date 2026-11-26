# Browser deployment

The project site is <https://takahirox.github.io/msx-rust/>.

## Application integration and reproducible build

Issue #4 integrates the existing application from [Issue #3](https://github.com/takahirox/msx-rust/issues/3)'s
`issue-3-browser-msx1` branch at commit
`7b0890f76fe62f47e4ac7841495812c933b3e635`, without waiting for its merge.
The core, browser UI and firmware sources are carried over unchanged; emulator
feature review remains in #3. Deployment integration pins the compiler, cleans
the output directory, and tests actual project-prefix hosting.

Install rustup, Node.js 22 or newer and Python 3. The workflow uses Node.js 22;
`rust-toolchain.toml` pins Rust 1.99.0 and installs `wasm32-unknown-unknown`.
Cargo and npm use the checked-in lockfiles. No wasm-bindgen, wasm-pack, Trunk or
frontend bundler is needed: the browser's `app.js` calls the exported Wasm API.

```sh
rustup show
# Keep Cargo/rustc/rustdoc consistent if a system Rust shadows rustup.
export PATH="$(rustup run 1.99.0 rustc --print sysroot)/bin:$PATH"
./scripts/build.sh
node scripts/check-dist.mjs
npm ci
npx playwright install chromium
npm test
```

On Linux CI, use `npx playwright install --with-deps chromium` to install browser
system libraries. GitHub's Ubuntu runner provides rustup and Python 3; `rustup show`
installs the toolchain specified by the repository before the build runs.

The build script compiles `msx-wasm` in release mode with `--locked`, recreates
`dist/` and assembles the Pages artifact. It contains 18 files.

- `index.html`, `style.css`, `app.js`, `display.js`, `saves.js`, `msx_wasm.wasm`
- `weave-boot.rom`, `weave-msx2-boot.rom`, `msx2-diagnostic-sub.rom`, `weave-disk.rom`
- `cbios_main_msx1.rom`, `cbios_main_msx2.rom`, `cbios_sub.rom`
- `LICENSE`, `ROM-LICENSE.txt`, `CBIOS-LICENSE.txt`, `CBIOS-PROVENANCE.txt`, `emu2413-LICENSE.txt`

Firmware is also embedded in Wasm. Normal boot and local file imports require
only the app's HTML, JS, CSS and Wasm requests. ROM, DSK and CAS media come from
the user. `npm test` generates local-only test media under
`target/browser-fixtures/`; Chromium imports them as files from disk while the
HTTP servers serve the same Pages artifact at both roots.
[Runtime asset inventory](../firmware/ROM-LICENSE.txt) · [Test fixture inventory](../tests/fixtures/README.md)

[CBIOS-PROVENANCE.txt](../firmware/cbios/CBIOS-PROVENANCE.txt) records the original
SourceForge release/archive URLs, version 0.29a, archive size and SHA-256, all three
ROM sizes/hashes and exact slot mapping.
[CBIOS-LICENSE.txt](../firmware/cbios/CBIOS-LICENSE.txt) reproduces the complete
License section from that release's `doc/cbios.txt`, with all copyright holders, both
redistribution conditions and disclaimer. Its binary redistribution condition requires
these notices in accompanying materials; retain both records with the site and
Wasm/ROM redistributions. C-BIOS remains separate from the repository's MIT license.
The Settings panel links both C-BIOS notices. The YM2413 synthesis adaptation
retains its upstream MIT notice in `emu2413-LICENSE.txt`, which accompanies native
and Wasm distributions. `scripts/check-dist.mjs` validates the exact file list,
Wasm imports and required exports, firmware headers and sizes, pinned C-BIOS
SHA-256 hashes, notice bytes and relative HTML links.

All module, stylesheet and Wasm URLs are relative; there is no client-side router.
Browser tests start two isolated Python static servers: the ordinary local site
on port 41873 and the unchanged `dist/` artifact at
`http://127.0.0.1:41874/msx-rust/`. The prefix test uses real HTTP requests, verifies
HTML/JS/CSS/Wasm loading beneath the prefix, boots BASIC, types `PRINT 42` and
checks the transcript and Canvas with no failed requests or page errors.

The MSX2 suite boots the default embedded C-BIOS pair and independent BIOS service
cartridge at both roots, verifying Canvas pixels, keyboard/PSG audio and silence,
local pair validation, firmware switching, privacy and MSX1 restoration. It also
boots the diagnostic cartridge and checks 512-wide Canvas output. See the
[current MSX2 validation record](msx2.md#verification); the results below record
the earlier deployment integration. Issue #15 additionally requires post-merge
verification of the actual published C-BIOS path, static firmware/notices and
service cartridge, with deployment run and observed results recorded; this remains
pending in the historical record. The service-test cartridge is now a locally generated fixture.

## Pre-merge validation

Validated locally on 2026-10-17, macOS arm64, Rust 1.99.0, Node.js 24.12.0 and
Playwright 1.64.1 / Chromium 156.0.8078.4:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 25 integration tests passed |
| `./scripts/build.sh` | Passed with the local macOS linker workaround below |
| `node scripts/check-dist.mjs` | Exact eight-file inventory, relative HTML links, Wasm API and ROM checks passed |
| `npm ci`; `npx playwright install chromium`; `npm test` | Seven browser tests passed, including the real `/msx-rust/` static-server test |
| `go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 .github/workflows/pages.yml` | Passed |
| Local documentation links; `git diff --cached --check` | 18 local link targets exist; no whitespace errors |

The BASIC screenshot was inspected and shows the entered program and visible
output 1–5. Wasm is 181,055 bytes with SHA256
`3d41eb3a179434b0369614716d5446634ef3bc40647267bbb8c654f40998779e`;
both generated ROM hashes also match #3's [validation record](validation.md).
The optional external C-BIOS test was not rerun for this deployment integration.

The initial macOS build failed because the freshly installed `rust-lld` could not
locate its own `libLLVM.dylib`. Exporting `DYLD_LIBRARY_PATH` before the shell build
did not fix that loader failure. The successful build used an ignored local
wrapper to set the library path immediately before executing the same pinned
Rust linker; no compiler, source or system installation was changed:

```sh
mkdir -p target
cat > target/pages-linker.sh <<'SH'
#!/bin/sh
toolchain_root="$(rustc --print sysroot)"
export DYLD_LIBRARY_PATH="$toolchain_root/lib"
exec "$toolchain_root/lib/rustlib/aarch64-apple-darwin/bin/rust-lld" "$@"
SH
chmod +x target/pages-linker.sh
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER="$PWD/target/pages-linker.sh" ./scripts/build.sh
```

Run this only after selecting the pinned toolchain in `PATH` as above. The Ubuntu
workflow uses the normal Rust linker. GitHub CI has not been rerun in this local
fix node; no push, repository setting change or production deployment was made.
Local builds and browser tests do not establish that production publishing works;
the live checks below remain pending.

## Demo ROMs validation (Issue #21)

Validated locally on 2026-10-20, macOS arm64, Rust 1.99.0, Node.js 24.12.0,
Playwright 1.64.1 / Chromium. The build used the same local linker wrapper
explained above; the workflow needs no change.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 84 tests passed |
| `./scripts/build.sh` | Passed; all game bytes are checked in, no third-party ROM downloads |
| `node scripts/check-dist.mjs` | Passed; exact 25-file deployment, pinned ROM and notice hashes, source attribution, relative URLs and Wasm exports |
| `npm test` | All 50 tests passed in 4.3 minutes, including both site roots and previous BASIC/mapper/audio/focus/fullscreen/mobile workflows |
| `npm test -- demos.spec.js --output target/demo-final-results` | All seven demo tests passed again in 29.5 seconds after final selector alignment and explicit game-Eject coverage |
| `git diff --cached --check` | Passed; upstream notice text preserved with documented whitespace normalization |

The demo tests execute the real compiled Wasm CPU/VDP, with deterministic frame
stepping for game assertions. They observe original game HUD/sprite pixels and
upward player movement from keyboard/joystick input, fire shots, reset both games,
switch cartridges/models, eject to same-model BASIC, and exercise local imports
and explicit firmware precedence. They reject missing/corrupt/invalid assets and
catalogs without losing the previous program or model; a delayed fetch cannot
replace a newer cartridge after Eject. Requests stay within the static site's
own root and no storage/upload/analytics path is used.

The screenshots and binary redistribution audit remain available in the
[pre-removal revision](https://github.com/takahirox/msx-rust/tree/ca1e8d0b60d142fcbfae8c07014454aee3d18ca0/docs/demo-evidence)
and [its provenance record](https://github.com/takahirox/msx-rust/blob/ca1e8d0b60d142fcbfae8c07014454aee3d18ca0/demos/PROVENANCE.txt).
The original live check was recorded as pending. Issue #87 retires the sample
catalog and replaces the deployed sample workflow with local media imports.

## One-time repository owner setup

In `takahirox/msx-rust`, open **Settings → Pages → Build and deployment → Source**
and select **GitHub Actions**. This enables Pages artifact deployments rather
than publishing files from a branch. Do not select **Deploy from a branch** or
create an additional Pages starter workflow. See GitHub's
[custom Pages workflow documentation](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages).

Ensure repository Actions settings permit the official `actions/*` actions used
by this workflow. Under **Settings → Environments → github-pages**, allow
deployments from `main`. If unattended deployment is required, the environment
must not require manual reviewer approval.

The workflow uses the normal `GITHUB_TOKEN`; no personal token is required.
It does not automatically enable Pages or change repository settings. The Pages
API returned HTTP 404 during implementation, so activation and publishing source
are unconfirmed and must be recorded by an owner or subsequent verifier.

## Triggers and failure inspection

After owner setup, every push to `main` builds, validates and uploads `dist/`
using `actions/upload-pages-artifact`. Only after the build and browser checks
succeeds does the deployment job run `actions/configure-pages` and
`actions/deploy-pages`, targeting the `github-pages` environment. Production
deployments share a concurrency group and do not cancel an active deployment.

Pull requests targeting `main`, including fork PRs, run the same build with
`contents: read`. They neither upload a Pages artifact nor run the deployment
job. Only the deployment job receives `pages: write` and `id-token: write`.

Open **Actions → Browser app / GitHub Pages → the relevant run** to inspect failed
steps. A build error prevents upload and deployment. For deployment errors, check
the Pages publishing source, environment rules and deployment job logs. The
`github-pages` environment and deployment step expose the actual published URL.

## Required post-merge verification (pending)

Follow the current [merge and follow-up lifecycle](development-flow.md#5-merge-and-report-any-required-follow-up): close the implemented source Issue after successful merge, then record the required deployment checks. Historical pending results below remain unverified; closure does not claim they passed. Track actual defects in linked follow-up Issues.

For Issue #87, after Pages succeeds, open the published player and record the
deployed revision, Actions run, browser and operating system. Record the following observations.

- MSX2 Weave BASIC boots, and the UI and network requests contain no sample media or catalog.
- The speaker can start sound with a gesture, mute it, and restore the previous volume.
- Switching tabs suspends a running session, and returning resumes it. A manual Pause survives the same tab changes.

Use a private original fixture if local media are needed. Record any blocked or
unperformed checks. The pre-merge browser tests drive visibility events and frame
timestamps deterministically while running real Wasm and Web Audio. Actual tab
switching on the published site is part of this post-merge check.

The checklist below records the original integration's historical deployment work.

After this integration is reviewed and merged:

- Record that Pages uses **GitHub Actions**, including any completed owner action.
- Observe a `main` push build and successful deployment; record its run URL and
  deployed commit.
- Open the URL reported by the deployment (expected target above), check HTML,
  JS and Wasm network requests under `/msx-rust/`, and verify that the emulator
  UI loads and boots. Track emulator feature gaps in #3.
- Observe a subsequent `main` update producing a successful updated deployment;
  record the run/commit and a verifiable deployed content change.

These checks have not been performed. Local builds and workflow linting cannot
establish that production publishing works.
