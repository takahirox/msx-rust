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

The Rust/Wasm emulator and browser test tooling have build and validation commands documented in [README.md](../README.md#development-and-verification). Run the checks relevant to implementation changes. Do not add unrelated tooling merely to document a process.

Choose checks appropriate to the changed files and actual repository tooling. For templates and documentation:

- Check Markdown structure, Issue-template YAML front matter, links and anchors, and consistency between templates and guidance, using available tooling and manual review as needed.
- Run `git diff --check` for whitespace errors on tracked changes; inspect newly added files as well.
- Verify that only the intended files changed with `git status --short` and review the diff.

Documentation-only changes do not require unrelated runtime suites, new runtime tests, hardware/device checks, or deployed-site verification. Use the documented runtime checks when implementation files change.

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
