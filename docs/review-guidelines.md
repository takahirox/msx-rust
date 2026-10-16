# Review and Merge Guidelines

Follow the [development flow](development-flow.md), including the English collaboration convention. These guidelines apply to contributors and AI agents.

## Review against the source Issue

Read the source Issue, its completion criteria, the PR description, the full diff, and validation evidence. The central review question is:

**Does the PR completely solve the source Issue, without unnecessary scope or complexity?**

Check both directions:

- **No missing work:** all required behavior and acceptance criteria the PR claims to resolve are implemented and appropriately validated. Do not approve a PR as closing an Issue with unresolved requirements. An intentionally partial PR must identify the remaining work and keep the Issue open.
- **No unnecessary work:** reject unnecessary abstractions, speculative extensibility, unrelated refactoring, frameworks/subsystems, or extra policies/configuration without a demonstrated need tied to the Issue.

Prefer the smallest coherent design that fully satisfies the Issue. Added complexity is not automatically beneficial. Preserve justified existing conventions; do not use simplicity to excuse omitted requirements or necessary validation.

## Check the result and evidence

Verify correctness, consistency with the repository, and documentation affected by the change. Choose validation appropriate to the changed files and actual tooling. Check that reported results accurately distinguish passed, failed, skipped, and unperformed checks.

There are currently no configured build/test commands. For templates and documentation, inspect Markdown syntax, Issue-template YAML front matter, links and anchors, whitespace, and consistency as described in the [validation guidance](development-flow.md#validation-in-this-repository). Do not require invented Cargo commands, new tooling, unrelated runtime suites, hardware checks, or deployed-site verification for documentation-only work.

## Separate pre-merge acceptance from later verification

Require all mandatory pre-merge acceptance criteria to be achievable, verifiable, and satisfied before merge. Keep optional checks separate. Human confirmation, physical-device testing, subjective evaluation, or external approval may be mandatory only when necessary for the actual Issue, with a reason and expected result.

Checks possible only after merge must not block pre-merge approval. Record genuinely necessary post-merge checks separately as pending until performed; do not add routine follow-up checks without an Issue-specific need. This distinction preserves all implementation requirements and applicable pre-merge checks.

For example, pre-merge builds and automated checks cannot prove a merge-triggered deployment or published site succeeded. Verify such results after merge when required. The initial collaboration templates and documentation require no post-merge verification.

## Choose a review outcome

- **Agent-fixable findings:** request concrete fixes, identify the affected file or requirement, and explain the expected result. Review the updated diff and relevant validation again after revision; repeat until blocking findings are resolved.
- **Necessary human input:** record the specific unanswered question, why human input is necessary, and the expected result. Mark the review as waiting for human input and use the workflow's wait path. If no automated wait mechanism is available, pause the affected work and report the question. Do not repeatedly request code fixes for this state or infer approval from elapsed time. Reassess after the answer arrives.
- **Ready to merge:** approve and merge within the task's authorization when the implementation fully meets the Issue, mandatory pre-merge checks pass, and no blocking findings or necessary human decisions remain.

Nonblocking suggestions and optional validation must be identified as such. Pending checks possible only after merge do not prevent approval when the merge conditions are met. After merge, report any required verification separately with actual results or a pending status; never claim deployment or publication succeeded solely from pre-merge evidence.
