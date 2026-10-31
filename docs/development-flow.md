# Development Flow

The development flow for `msx-rust` is: Issue → implementation → PR → review → revision as needed → merge. Perform post-merge verification only when the Issue requires it.

## Collaboration language

Use **English** for newly authored templates, development/review/merge documentation, AI instructions, Issue/PR titles and bodies, review comments, and commit messages. This collaboration convention does not require translating historical content or changing product localization.

## 1. Start with an Issue

Use the [Issue template](../.github/ISSUE_TEMPLATE/issue.md) to capture the problem, expected outcome, relevant context, reproduction steps where applicable, and concrete completion criteria. No assignee or owner field is required.

Treat the source Issue as the scope of the work. Clarify genuinely blocking ambiguity before implementing the affected work; do not invent requirements.

Default to completion criteria an AI agent can execute and verify. Require human confirmation, physical-device testing, subjective evaluation, or external approval only when necessary for the actual Issue. Explain why human work is necessary and what result is expected.

Separate these categories:

- **Pre-merge acceptance:** mandatory criteria achievable and verifiable before merge, including all implementation requirements and applicable checks.
- **Optional validation:** useful additional checks that are not mandatory completion criteria.
- **Required post-merge verification:** genuinely necessary checks possible only after merge. State "None" when none are needed; do not add them routinely.

Checks possible only after merge must not be pre-merge approval conditions. For example, a merge-triggered deployment can only be verified after merge; any applicable code/configuration checks and local builds belong before merge. Keep required post-merge verification pending until performed.

## 2. Implement the smallest complete change

Inspect the current repository and default branch, `main`, before choosing an approach. Preserve useful existing content and justified conventions. Prefer the smallest coherent design that fully satisfies the Issue. Avoid unnecessary abstractions, speculative extensibility, unrelated refactoring, frameworks/subsystems, and extra policies or configuration without a demonstrated need tied to the Issue.

Simplicity must not excuse omitted requirements or necessary validation. Added complexity is not automatically beneficial.

### Validation in this repository

The Rust/Wasm emulator and browser test tooling have build and validation commands documented in [README.md](../README.md#development-and-verification). Run the checks relevant to implementation changes, following the [test and CI performance principles](#test-and-ci-performance-principles). Do not add unrelated tooling merely to document a process.

Choose checks appropriate to the changed files and actual repository tooling. For templates and documentation:

- Check Markdown structure, Issue-template YAML front matter, links and anchors, and consistency between templates and guidance, using available tooling and manual review as needed.
- Run `git diff --check` for whitespace errors on tracked changes; inspect newly added files as well.
- Verify that only the intended files changed with `git status --short` and review the diff.

Documentation-only changes do not require unrelated runtime suites, new runtime tests, hardware/device checks, or deployed-site verification. Use the documented runtime checks when implementation files change.

### Test and CI performance principles

**Detect the required failures at the lightest appropriate layer, without redundant execution.** Preserve required correctness, regressions and meaningful integration coverage when moving or consolidating checks.

1. **Prefer the lightest meaningful layer.** Use Rust unit/component tests for calculations, branches and broad data combinations. Use native Machine tests for emulated CPU/device interactions, and compiled-Wasm/Chromium tests where browser integration, Canvas, Web Audio, file inputs or lifecycle behavior require them. Add assertions that catch a concrete defect rather than mirror the implementation. Keep independent hardware/reference expectations where they supply confidence that agreement between layers cannot.
2. **Choose combinations deliberately.** Select representative MSX1/MSX2, firmware, mapper/media, video-mode, viewport and hosting-root cases covering distinct paths. Exhaust combinations when their interactions require it; do not automatically multiply every dimension. Preserve required regressions and the meaningful integration coverage of local `/` and Pages `/msx-rust/` hosting when redistributing checks.
3. **Match validation to the change.** Documentation/templates use the checks above. UI changes need relevant Chromium interaction/layout checks; core/runtime changes need Rust formatting, Clippy and tests plus affected Wasm/browser boundaries. CI/build changes need configuration review and affected build, inventory and browser checks. Focused checks give early feedback; full Rust/browser suites provide regression confidence before merge where applicable. The [Pages workflow](../.github/workflows/pages.yml) currently runs formatting, Clippy, Rust tests, `./scripts/build.sh`, `node scripts/check-dist.mjs` and `npm test` for PRs targeting `main` and pushes to `main`, without path-based selection. These configured checks remain applicable pre-merge requirements; local selection does not waive them. Optional external-firmware/reference comparisons remain optional unless the Issue requires them. Pages artifact upload/deployment runs only on `main` pushes; record genuinely required live-site verification separately after merge, as in the [deployment guide](deployment.md#required-post-merge-verification-pending).
4. **Reuse work and avoid duplicate execution.** Reuse compatible Wasm builds, generated fixtures, downloads and setup with cache keys covering toolchain, lockfiles and relevant inputs, while isolating mutable test state. The workflow already caches npm downloads and tests the built `dist/` artifact before deployment. Avoid equivalent push/PR runs for the same revision when evolving triggers; a new merge revision still needs validation. Cancel superseded validation when safe, preserving production deployment ordering (the current production group does not cancel active deployments). Measure elapsed time and resource use before increasing parallelism; Chromium currently uses one worker and isolated local servers.
5. **Review value and cost together.** Identify the additional defect each new/expanded test detects, whether existing coverage already supplies that assurance, and its execution/maintenance cost. Watch repeated fresh browser, renderer or compiler setup, large ROM/disk fixtures, long emulation loops and Cartesian-product growth. Retain meaningful assertions and required coverage when reducing repetition.
6. **Maintain an execution-time budget from evidence.** Historical [local validation results](validation.md#mandatory-checks) include 206 browser cases in 15.0 minutes on macOS; this is not a normal-CI baseline. Normal-CI timing and a target remain to be established during actual performance work. Record the revision, environment and check scope, separate elapsed feedback time, total runner/computation time and queue time, then monitor growth and investigate work exceeding the established target. Revisit validation structure and duplication while retaining required confidence. This guidance adds no benchmark, hardware purchase or merge gate.

Changes to test selection, workflow triggers, caching, timeouts or worker counts require separately scoped implementation work and applicable validation.

## 3. Open a PR for the Issue

Target `main` by default and use the [PR template](../.github/pull_request_template.md). Explain the problem, resulting behavior, focused changes, related Issues, and validation actually performed. Report failures, skipped checks, and unperformed checks accurately with reasons; do not imply they passed.

Claim to close the source Issue only when the implementation fully addresses it. If intentionally partial, identify remaining requirements and keep the Issue open. Record required post-merge checks separately as pending, or state "None." Pending checks possible only after merge do not make an otherwise complete implementation partial.

Use concise English commit messages describing what changed and why, referencing the Issue.

## 4. Review and revise

Review every PR using the [review guidelines](review-guidelines.md). The central question is: **Does the PR completely solve the source Issue, without unnecessary scope or complexity?** Check both missing requirements and unjustified additions.

Request fixes for agent-fixable findings, then review the updated PR again. When genuinely necessary human input is missing, record the specific question and expected result, and wait through the workflow's wait path. Keep that state distinct from fixable findings; do not repeatedly ask for code changes to resolve a human decision.

## 5. Merge and report any required follow-up

Approve and merge when the implementation is complete, mandatory pre-merge checks pass, and no blocking review findings remain. Act within the current task's authorization and repository permissions.

If required, perform recorded post-merge verification after merge and report actual results, including failures and follow-up work. Keep unperformed checks pending. Pre-merge validation does not prove deployment or publication succeeded. No post-merge verification is required for the initial collaboration templates and documentation.
