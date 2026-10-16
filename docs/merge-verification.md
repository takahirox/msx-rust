# Issue #3 merge and publication verification

Verified on 2026-10-16 UTC (2026-10-17 in Asia/Tokyo).

- Source: <https://github.com/takahirox/msx-rust/issues/3>.
- PR: <https://github.com/takahirox/msx-rust/pull/6>.
- Reviewed head: `88708aea629da33197115819be49be4106e20f80`.
- Merge commit: `dad4e7769bd51f68ea3e6c24b7dc2e11d6c8eb75`.

Immediately before the merge attempt, the source Issue was read from GitHub and
confirmed open. The PR had the reviewed head, targeted `main`, and had passed its
build check with a clean merge state. The completed upstream review reported no
blocking findings. The outdated draft/listening gate in the PR description was
updated to match the Issue's revised acceptance criteria, and the PR was marked
ready. No required checks or reviews were bypassed.

GitHub accepted a merge using `merge_method=merge` and the expected head SHA.
The resulting commit has parents `618911c8eeabd4ab0f653a35f0ec43d19da0637a`
and `88708aea629da33197115819be49be4106e20f80`, preserving the PR branch history.

## Required post-merge verification

[The merge-triggered workflow](https://github.com/takahirox/msx-rust/actions/runs/38928454212)
ran against the merge commit. Its **Build browser app** job passed formatting,
Clippy, Rust tests, the Wasm build, deployment inventory, Chromium browser tests,
and production artifact upload.

The **Deploy GitHub Pages** job failed at **Configure GitHub Pages**:

> Get Pages site failed. Please verify that the repository has Pages enabled and configured to build using GitHub Actions

The action reported HTTP 404, as did the repository Pages API. After the workflow
completed, <https://takahirox.github.io/msx-rust/> also returned HTTP 404.
No successful publication or live emulator behavior is claimed.

Owner prerequisite: in **Settings → Pages → Build and deployment → Source**,
select **GitHub Actions**, and ensure the `github-pages` environment permits
deployment from `main`. Repository settings were not changed by this merge node.
After setup, rerun the failed deployment job and confirm successful publication.

Live Wasm loading, interactive BASIC, keyboard input, local ROM selection, and
audio activation remain pending because the site is unavailable. Once deployed,
perform the Issue's automated browser checks against the published URL and record
their actual results. Human listening remains an optional post-merge owner check.
