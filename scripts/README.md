# scripts

Helpers for the pull request workflow (they need the `gh` CLI):

| Script | Does |
| --- | --- |
| `pr-create [--breaking] <title> <body-file>` | Pushes the branch and opens a PR; `--breaking` adds the label at creation so the semver job sees it. Refuses if the branch is behind `origin/main`. |
| `pr-wait <pr> [seconds]` | Waits for the PR's checks. Exit 0: all passed, 1: a check failed, 3: timed out. |
| `pr-merge <pr>` | Rebase-merges a PR whose checks all passed, updates `main`, deletes the local and remote branch. |
| `issue-body get <n>` / `set <n> <file>` | Reads or replaces an issue or PR body through the REST API (`gh pr edit` fails here). |
