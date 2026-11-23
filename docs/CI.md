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
check pending forever and blocks merges. The required checks on `main` are listed under
[The `main` ruleset](#the-main-ruleset) below.

| Workflow | Job (required check) | Runs when these change |
|---|---|---|
| `ci` | `host crates (fmt, clippy, test)` | `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `crates/**`, `ci.yml` |
| `ci` | `shellcheck` (also file modes and the reviewer self-test) | `scripts/**`, `tests/**`, `antigravity-review.yml`, `ci.yml` |
| `ci` | `patch series + armv7 daemon build` (runs `tests/apply-patches.sh` first) | `upstream` (submodule pointer), `.gitmodules`, `patches/**`, `scripts/apply-patches.sh`, `scripts/build.sh`, `tests/apply-patches.sh`, `ci.yml` |
| `docs` | `markdownlint` | any `*.md`, markdownlint config, `docs.yml` |
| `docs` | `links` | any `*.md`, `lychee.toml`, `docs.yml`, and any deletion or rename anywhere (link targets are not only Markdown) |
| `security` | `gitleaks` | always (fast; a secret can land in any file) |
| `security` | `cargo-audit` (pinned `cargo-audit` 0.23.0 CLI, `--deny warnings`) | Cargo manifests or lockfile, `security.yml` |
| `security` | `codeql` | `crates/**`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.github/**` |

A workflow file change re-runs that workflow's jobs. `workflow_dispatch` runs every job. The
weekly `schedule` runs of `docs` (link rot) and `security` (new advisories and CodeQL queries)
also run every job, regardless of changes.

Examples: a README-only push runs `markdownlint`, `links` and `gitleaks`; a `patches/` change
runs the armv7 build, `gitleaks` and nothing else.

## Caching

| Cache | Scope | Key |
|---|---|---|
| Host crates (`Swatinem/rust-cache`, `shared-key: host`) | `target/`, cargo registry | toolchain + `Cargo.lock` + `CARGO_*` env |
| Patched upstream (`Swatinem/rust-cache`, `shared-key: patched`) | third-party dependencies built into `build/rayhunter/target` (workspace crates are stripped) | toolchain + staged upstream `Cargo.toml`/`Cargo.lock` + `CARGO_*` env; deliberately not the patch series |
| rustup toolchain + armv7 target (`actions/cache`) | `~/.rustup/toolchains` | upstream `rust-toolchain.toml` |
| npm (`setup-node` cache) | npm download cache | `build/rayhunter/package-lock.json` |
| gitleaks binary (`actions/cache`) | `~/.local/bin/gitleaks`, SHA-256-verified before extraction | pinned version |
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
- Isolation (template hardening, 2026-11-22): agy runs with `--dangerously-skip-permissions` and
  reads an untrusted diff, so it is started from a private `mktemp -d` sandbox under
  `$RUNNER_TEMP`, never from the checkout. The large-diff handoff file is written into that
  sandbox and only the sandbox is passed to `--add-dir`. agy gets an allowlisted environment
  (`env -i` plus `HOME`, `PATH`, locale, `TERM`, the `XDG_*` directories,
  `DBUS_SESSION_BUS_ADDRESS` and display variables, which is what its keyring-held Google session
  needs); no job token, `ACTIONS_*` variable or workflow `env:` value reaches it, whatever its
  name. The script's post-agy `git` calls disable hooks and fsmonitor. Verified live on PR #1
  from a systemd user unit with planted `ACTIONS_RUNTIME_TOKEN` and `DATABASE_URL` canaries:
  agy's working directory and its only `--add-dir` were the sandbox and its environment held
  exactly the 12 allowlisted variables present on the host. `agy-review-selftest.sh`
  asserts all of this. `AGY_DIFF_MODE` is back to the template default (`auto`), so large diffs
  are reviewed whole again.
- Jail (`AGY_ISOLATION=bwrap`, default): agy's own `--sandbox` was verified not to confine
  reads, and agy otherwise loads the host's MCP servers, skills and hooks. So agy runs in a
  bubblewrap jail with an empty private `HOME`, read-only `/usr` and `/etc`, and only the review
  sandbox writable. Its D-Bus is an `xdg-dbus-proxy` (`--filter`) that allows `GetSecret` only
  on agy's own Secret Service item; the item is resolved outside the jail from
  `AGY_KEYRING_ATTRS`, and an empty attribute set is refused.
- Egress (`AGY_EGRESS=filtered`, default): the jail has no network of its own. Its only route
  out is an HTTPS `CONNECT` proxy outside the jail that accepts port 443 of exact
  `AGY_EGRESS_ALLOW` hosts only, with no wildcards. Google's front end routes by the `Host`
  header on any googleapis connection, so the proxy terminates TLS for **every** allowed host
  with a per-run CA (key kept outside the jail). It carries one HTTP/1.1 request per connection
  and rebuilds it with its own `Host` header, forwarding the body exactly once. A host in
  `AGY_EGRESS_PATH_RULES` (default `www.googleapis.com=/oauth2/v2/userinfo`, because that host
  also serves the Cloud Storage API) accepts only a body-less `GET` of exactly that path. The job
  log shows egress verdict counts only. Denied host names go to
  `~/.gemini/antigravity-cli/agy-egress-denied.log` on the runner host (mode 600); after an
  agy update starts failing reviews, look there and add the needed host to `AGY_EGRESS_ALLOW`.
- Fail closed: a runner host without `bwrap`, `xdg-dbus-proxy`, `gdbus`, `socat`, `python3`
  (3.11 or later), `openssl` or a system CA bundle, without a user session, or with no matching
  keyring item refuses to run agy. A jail that fails to start ends the run with nothing posted.
  Credential-shaped strings in agy's output are redacted to `[REDACTED: <kind>]` and the comment
  says so; only a match that redaction cannot remove stops the post. A credential-shaped stderr
  tail is withheld from the job log.
- The template's first-install bootstrap steps were removed before the reviewer reached `main` (it
  bootstrapped itself once on PR #1):
  they check out PR-head code in the job and trip CodeQL's `actions/untrusted-checkout` (high).

## Adding a job

Give it `needs: changes` and an `if:` on a filter output (add the filter to the `changes` job),
include `github.event_name == 'workflow_dispatch'` (or the non-push/PR clause used in `docs` and
`security`) so manual runs still execute it, pin every action to a commit SHA, and set
`persist-credentials: false` on checkout. If the job should block merges, add its name to the
required checks of the `main` ruleset after its first successful run.
