# scripts

Helpers for the pull request workflow (they need the `gh` CLI):

| Script | Does |
| --- | --- |
| `pr-create [--breaking] <title> <body-file>` | Pushes the branch and opens a PR; `--breaking` adds the label at creation so the semver job sees it. Refuses if the branch is behind `origin/main`. |
| `pr-wait <pr> [seconds]` | Waits for the PR's checks. Exit 0: all passed, 1: a check failed, 3: timed out. |
| `pr-merge <pr>` | Rebase-merges a PR whose checks all passed, updates `main`, deletes the local and remote branch. It only moves the checkout when it is on the PR's branch with no uncommitted changes. |
| `issue-body get <n>` / `set <n> <file>` | Reads or replaces an issue or PR body through the REST API (`gh pr edit` fails here). |
| `bundle-types [--clean]` | Copies the type library and the gallery's `.ksy` files into `crates/doe-cli/bundled/` (git-ignored, listed in the package) so that a crate published to crates.io embeds them; run it before `cargo publish -p doe-cli`. In a checkout `build.rs` reads them where they are. |
| `wasm-test` | Builds the library for `wasm32-unknown-unknown` without the filesystem and runs `tests/limits.rs` as `wasm32-wasip1` in Node's WASI (`wasm-runner.mjs`); the `wasm` CI job runs it. |
| `coverage [--report-only]` | Measures the line coverage of the workspace's tests with cargo-llvm-cov and fails below the threshold written in the script (see the README, *Test coverage*); the `coverage` CI job runs it. |
