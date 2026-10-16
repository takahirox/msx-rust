# Browser deployment

The target project site is <https://takahirox.github.io/msx-rust/>.
This is a target URL, not a verified working deployment.

## Current integration status

Issue #4 establishes the deployment workflow in parallel with [Issue #3](https://github.com/takahirox/msx-rust/issues/3).
The current checkout and GitHub `main` contain no application, Rust manifest,
browser build tooling, or configured build commands. No application branch or PR
was available when this workflow was added.

The **Build integration required (Issue #3)** step deliberately fails with an
Actions error. This prevents a green build or a placeholder deployment from
being mistaken for a working emulator. Issue #4 remains incomplete until the
real application is integrated and its required pre-merge checks pass.

## Complete the application integration

Coordinate through #3's PR when its build tooling becomes available; #3 does not
need to merge before this integration can be implemented and tested. Keep emulator
implementation and feature validation in #3.

1. Replace the explicit failing step in
   [the workflow](../.github/workflows/pages.yml) with #3's actual dependency setup
   and build command. Pin the Rust toolchain, Wasm/browser tools and dependencies
   according to that implementation, use its lockfiles, and document the exact
   local command here. No Cargo, wasm-pack, Trunk or Node build command has been
   selected or assumed by this deployment scaffold.
2. Stage only the generated static website in `_site/`, the deployment staging
   directory chosen here. This is not an assertion about #3's output layout:
   either explicitly copy its website output or update the artifact path to its
   actual clean output directory. Start from clean output and include entry HTML,
   JavaScript bindings, Wasm and required lawful assets. Never copy the repository
   root, source, tests, user ROMs or proprietary BIOS/BASIC/game ROMs. Review the
   provenance and redistribution permission for every bundled asset.
3. Use relative asset/module/Wasm URLs or configure the actual build tool's public
   base as `/msx-rust/`. Configure any router for that prefix. A leading `/pkg/`
   or `/app.js` would resolve at the domain root and break project hosting.
4. Run the real build locally. Inspect the complete output inventory and ensure
   the entry HTML, JS and Wasm are present and nonempty. Serve the output at
   `/msx-rust/` on a local static server, open it in a current desktop browser,
   and check that the UI boots and HTML, JS, Wasm and required assets load without
   console or network errors. Serving only at `/` does not validate the prefix.
   Record actual commands and results in the integration PR. Compilation alone
   does not prove that the browser app works.

The exact build command, dependency setup, generated artifact checks and local
browser smoke test remain pending; they cannot be validated against absent
application files. Do not remove the failing step until these are implemented.

## Scaffold validation

- Passed workflow linting with
  `go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 .github/workflows/pages.yml`.
- Checked parsed workflow triggers, permissions, production-only upload/deploy
  conditions, build dependency, environment, concurrency and staging path.
- Executed the current build step locally: it emitted the expected Actions error
  and exited with status 1; no website output was produced.
- Checked local documentation links and `git diff --check`.

These checks validate only the deployment scaffold. Actual application build,
artifact generation and static-server/browser checks are still pending.

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

Once build integration and owner setup are complete, every push to `main` builds
and uploads `_site/` using `actions/upload-pages-artifact`. Only after the build
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

After the real build integration is reviewed and merged:

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
