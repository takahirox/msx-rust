<!-- Write the title and body in English. See the development guidance:
https://github.com/takahirox/msx-rust/blob/main/docs/development-flow.md -->

## Summary

Explain the problem and the focused changes that address it.

## Outcome

Describe the resulting behavior, with a before/after example when useful. Identify any implementation or pre-merge acceptance left unresolved. Intentional partial work must state the remaining scope and keep the parent Issue open without an automatic closing reference. Pending post-merge-only checks do not make a complete implementation partial.

## Validation

Report checks actually performed and their results, including failures. Explain skipped or unperformed checks. Use checks appropriate to the changed files and existing repository tooling; do not invent build/test commands or require unrelated runtime suites for documentation changes.

## Required post-merge verification

List required checks possible only after merge as pending, or state "None." Keep these separate from pre-merge validation and optional checks. Do not claim deployment or publication succeeded from pre-merge evidence.

Follow the [merge and follow-up lifecycle](https://github.com/takahirox/msx-rust/blob/main/docs/development-flow.md#5-merge-and-report-any-required-follow-up): close the implemented Issue after successful merge, then record required checks in this PR, the closed Issue, or a linked validation report. Report pending or environment-blocked checks honestly; use linked follow-up Issues for actual defects.

## Related Issues

Link the source Issue and any other relevant Issues. Use and preserve `Closes #<number>` when this PR fully implements the source Issue and applicable mandatory pre-merge acceptance is satisfied, even if post-merge-only verification is pending.
