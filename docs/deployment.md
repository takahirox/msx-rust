# Browser deployment

The target project site is <https://takahirox.github.io/msx-rust/>.
This is a target URL, not a verified working deployment.

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
`dist/`, copies only the web entry files and license notices, then generates the
seven original boot/test ROMs from Rust source. It also copies only the three pinned
C-BIOS 0.29a ROMs and their separate license/provenance records from `firmware/cbios/`.
It also copies the checked-in `demos/` directory without network access.
The complete deployable inventory is **25 files**:

- `index.html`, `style.css`, `app.js`
- `msx_wasm.wasm`
- `weave-boot.rom`, `test-cartridge.rom`
- `weave-msx2-boot.rom`, `msx2-diagnostic-sub.rom`, `msx2-test-cartridge.rom`
- `cbios-test-cartridge.rom`, `cbios-msx1-test-cartridge.rom`
- `cbios_main_msx1.rom`, `cbios_main_msx2.rom`, `cbios_sub.rom`
- `LICENSE`, `ROM-LICENSE.txt`, `CBIOS-LICENSE.txt`, `CBIOS-PROVENANCE.txt`
- `demos/manifest.json`, `demos/PROVENANCE.txt`, `demos/plustakermsx.rom`, `demos/yokosht.rom`
- `demos/PLUSTAKER-LICENSE.txt`, `demos/YOKO-LICENSE.txt`, `demos/Z88DK-LICENSE.txt`

The [asset provenance inventory](../firmware/ROM-LICENSE.txt) identifies the original
MIT-licensed firmware, glyphs and cartridges, and separately BSD-2-Clause C-BIOS. No
proprietary BIOS/BASIC or commercial games are shipped. The two third-party
MSX1 demo games retain their own MIT licenses and z88dk runtime notice; see the
[per-binary and asset redistribution audit](../demos/PROVENANCE.txt) and
[pinned manifest](../demos/manifest.json). No npm packages are shipped.
The three unmodified official release binaries are also embedded in the core/Wasm, so automatic cartridge boot in either model needs no firmware network
request or SourceForge access. All assets can be served locally without external
network access; browser tests never download firmware at runtime.

[CBIOS-PROVENANCE.txt](../firmware/cbios/CBIOS-PROVENANCE.txt) records the original
SourceForge release/archive URLs, version 0.29a, archive size and SHA-256, all three
ROM sizes/hashes and exact slot mapping.
[CBIOS-LICENSE.txt](../firmware/cbios/CBIOS-LICENSE.txt) reproduces the complete
License section from that release's `doc/cbios.txt`, with all copyright holders, both
redistribution conditions and disclaimer. Its binary redistribution condition requires
these notices in accompanying materials; retain both records with the site and
Wasm/ROM redistributions. C-BIOS remains separate from the repository's MIT license.
No logo, BASIC, music or disk ROM is needed or distributed. The app links the notices
and downloadable independent service-test cartridge under advanced options.
`scripts/check-dist.mjs` rejects missing, empty or unexpected output files and checks
Wasm exports, ROM headers/sizes, exact pinned C-BIOS SHA-256 hashes, unchanged
license/provenance records and relative HTML links. It also validates the exact
demo inventory, ROM hashes/sizes and byte-identical license/provenance files, plus
required source attribution. Missing, extra or modified assets fail the check.
Only `dist/` is uploaded by the Pages workflow.

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
pending until performed on the deployed build.

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

Inspected screenshots retained in this commit:
[plustaker](demo-evidence/plustaker.png), [YOKO](demo-evidence/yoko.png),
[MSX2 SCREEN 8](demo-evidence/msx2.png), and
[320px Pages-prefix layout](demo-evidence/mobile.png).
Both games show running scenes after input; the diagnostic shows the emulated
color ramp, sprite and command output. The suite also verifies SCREEN 7 output
and restoration through Reset. Full captures remain in `test-results/demo-*.png`
and the CI `browser-evidence` artifact when the workflow runs.

Official game ROMs were compared byte-for-byte with the pinned upstream Git
objects. Each retained upstream license was checked for complete copyright,
permission and disclaimer text. Whitespace normalization is recorded in the
[redistribution audit](../demos/PROVENANCE.txt); game ROMs are unmodified.
Live deployment was subsequently verified after merge; see the
[Issue #21 verification record](demo-live-evidence/README.md) for the Actions
run, deployed commit, execution/input checks and published-site screenshots.

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

## Required post-merge verification

Issue #21's required Demo ROMs verification is complete on the actual deployed
site. The [verification record](demo-live-evidence/README.md) identifies the
successful Actions run and deployed commit and records game/diagnostic video,
working input, Reset/Eject/BASIC restoration and all manifest ROM/license URL
checks. No live-site defects were found requiring follow-up Issues.

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
