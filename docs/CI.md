# Continuous Integration

How the GitHub Actions workflows decide what to run, what they cache, and how that interacts with
the `main` repository ruleset.

## Path-aware jobs

Each of `ci.yml`, `docs.yml` and `security.yml` starts with a `detect changes (<workflow>)` job
(`dorny/paths-filter`, pinned) that classifies the diff of a push or pull request. Every other job
declares `needs: changes` and an `if:` on the relevant output, so it is **skipped** when its
inputs did not change.

Skipping is done with job-level `if:`, never with trigger-level `paths:` filters. A skipped job
still reports its check run as passing, while a workflow that never starts leaves a required
check pending forever and blocks merges. The required checks on `main` are listed in
[PROJECT-STATUS.md](../PROJECT-STATUS.md).

| Workflow | Job (required check) | Runs when these change |
|---|---|---|
| `ci` | `host crates (fmt, clippy, test)` | `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `crates/**`, `ci.yml` |
| `ci` | `shellcheck` (also file modes and the reviewer self-test) | `scripts/**`, `antigravity-review.yml`, `ci.yml` |
| `ci` | `patch series + armv7 daemon build` | `upstream` (submodule pointer), `.gitmodules`, `patches/**`, `scripts/apply-patches.sh`, `scripts/build.sh`, `ci.yml` |
| `docs` | `markdownlint`, `links` | any `*.md`, markdownlint or lychee config, `docs.yml` |
| `security` | `gitleaks` | always (fast; a secret can land in any file) |
| `security` | `cargo-audit` | Cargo manifests or lockfile, `security.yml` |
| `security` | `codeql` | `crates/**`, `Cargo.toml`, `.github/**` |

A workflow file change re-runs that workflow's jobs. `workflow_dispatch` runs every job. The
weekly `schedule` runs of `docs` (link rot) and `security` (new advisories and CodeQL queries)
also run every job, regardless of changes.

Examples: a README-only push runs `markdownlint`, `links` and `gitleaks`; a `patches/` change
runs the armv7 build, `gitleaks` and nothing else.

## Caching

| Cache | Scope | Key |
|---|---|---|
| Host crates (`Swatinem/rust-cache`) | `target/`, cargo registry | toolchain + `Cargo.lock` |
| Patched upstream (`Swatinem/rust-cache`) | `build/rayhunter/target` | toolchain + upstream `Cargo.lock` + hash of `patches/**` |
| rustup toolchain + armv7 target (`actions/cache`) | `~/.rustup/toolchains` | upstream `rust-toolchain.toml` |
| npm (`setup-node` cache) | npm download cache | `build/rayhunter/package-lock.json` |
| gitleaks binary (`actions/cache`) | `~/.local/bin/gitleaks` | pinned version |
| lychee results | `.lycheecache`, 1-day max age | rolling |

Only runs on `main` save Rust caches (`save-if`); pull requests and releases restore them but
never write, so caches do not churn across branches. Each Rust cache uses a `shared-key`
(`host`, `patched`) instead of the default job-ID key, so `release.yml` restores the same cache
`ci.yml` saves; the release workflow repeats the same workspaces, key and `CARGO_*` environment.

`scripts/apply-patches.sh` restages sources by content (`rsync --checksum`) and never touches
`build/rayhunter/target` or `node_modules`, so a restored cache survives `scripts/build.sh`.
Only changed files get a new mtime; that keeps Cargo's fingerprints correct (a reverted file is
rebuilt, an unchanged one is not), where restoring the archive's commit-time mtimes could make
Cargo reuse a stale patched artifact.

## The `main` ruleset

`main` is governed by a repository ruleset named `main` (Settings > Rules > Rulesets), which
replaced the classic branch protection rule on 2026-11-22. Rules:

- Restrict deletions; block force pushes; require linear history.
- Require a pull request (0 approvals, review threads resolved, squash merge only).
- Require these status checks, each pinned to the GitHub Actions app (integration 15368) so no
  other app can satisfy a check by posting a status with the same name:
  `detect changes (ci)`, `detect changes (docs)`, `detect changes (security)`,
  `host crates (fmt, clippy, test)`, `shellcheck`, `patch series + armv7 daemon build`,
  `markdownlint`, `links`, `gitleaks`, `cargo-audit`, `codeql`.
  The three detection jobs are required so the gating fails closed: if detection errors or times
  out, its dependents are skipped (which GitHub counts as passing), but the detection check itself
  fails and blocks the merge.
- Bypass: the repository admin role, mode "always". This is intentional: the maintainer may push
  directly to `main`, and such pushes are checked by CI after they land rather than before.
  Contributors go through pull requests.

Inspect with `gh api repos/doublegate/Orbic-Rayhunter/rules/branches/main`.

## Concurrency and limits

- Pull-request runs cancel superseded runs on the same branch; pushes to `main` always finish
  so every commit on `main` has a result.
- Every job has a `timeout-minutes` bound (5 to 60 minutes).
- Build artifacts (`orbic-rayhunter-devel`) are kept for 14 days.

## Antigravity PR reviewer

`antigravity-review.yml` reviews every same-repository PR on open, reopen and each push, and on a
`/agy-review` comment from a user with write access. It runs on this machine's self-hosted runner
(label `agy`, unit `github-agy-runner@orbic-rayhunter`) through the `agy` CLI's OAuth session, so
it uses the Google AI Ultra plan rather than a metered API key. It posts one comment per PR and
edits it in place, archiving earlier rounds. It is not a required check: it depends on the runner
host being online.

- Operations, configuration and security model: [.github/AGY-REVIEWER.md](../.github/AGY-REVIEWER.md).
- Project review rules: [.github/agy-review.md](../.github/agy-review.md).
- Fork PRs require approval for every outside contributor before any workflow runs, because the
  runner is a persistent self-hosted host.
- The scripts are vendored from the `antigravity-pr-review` template and kept byte-identical; the
  workflow runs the copies on the default branch, so script changes take effect only after merge.
- Local hardening: `AGY_DIFF_MODE: inline`. The template's file-handoff mode passes the whole
  checkout to agy with `--add-dir`, which, combined with `--dangerously-skip-permissions` and an
  untrusted diff, would let a prompt injection write `.git/hooks` or `.git/config` that a later
  token-bearing `git` call executes. Inline mode never exposes the checkout; very large diffs are
  truncated instead (logged). Open item for the template: write the handoff file outside the
  repository and scrub agy's environment by allowlist (`ACTIONS_RUNTIME_TOKEN` is still inherited).
- The template's first-install bootstrap steps were removed once the reviewer reached `main`:
  they check out PR-head code in the job and trip CodeQL's `actions/untrusted-checkout` (high).

## Adding a job

Give it `needs: changes` and an `if:` on a filter output (add the filter to the `changes` job),
include `github.event_name == 'workflow_dispatch'` (or the non-push/PR clause used in `docs` and
`security`) so manual runs still execute it, pin every action to a commit SHA, and set
`persist-credentials: false` on checkout. If the job should block merges, add its name to the
required checks of the `main` ruleset after its first successful run.
