# Report: the GitHub issue mirror and the pricing step, 2026-09-27

What was built, how it was checked, and what is left open. The design and
its trust boundaries are in [`docs/github-agent.md`](../github-agent.md).

## Commits

Each commit builds and passes the CLI's tests on its own.

1. `cli: prodrome github, a webhook payload as the events it means` — the
   verb, the `/price` grammar (`40`, `30 --deadline D`, `ref gh-7`,
   `accept`), `--proposal`, and unit tests for the grammar, the login rule
   and the association rule. The deadline defaults move to named constants on
   `command::Price` so `revise --deadline` and `/price --deadline` write one
   decay.
2. `cli: fixture payloads for prodrome github, and the mirror's laws` — 17
   trimmed webhook payloads and two replies under
   `cli/tests/fixtures/github/`, driven through the command line, and two
   property tests.
3. `cli: date a pricing-bot claim no earlier than what it rests on` — a fix
   found by running the workflow's steps locally (below).
4. `github: mirror.yml, issues and /price comments onto roadmap-data`.
5. `github: price.yml, Claude proposes a price as a pricing-bot claim`.
6. `docs: the issue mirror and pricing bot, and where trust stops` — README,
   CHANGELOG (Unreleased), `docs/github-agent.md`.
7. This report.

## Gates

| Gate | Result |
|---|---|
| `nix flake check` (core tests, CLI build and tests, clippy `-D warnings`, wasm build) | pass ("all checks passed!") at `fccc5e4`, the last commit touching code; the docs commits after it are outside the flake's source fileset. Inside the sandbox the CLI ran 17 unit, 18 `github` and 14 verb tests |
| `cargo test -p prodrome-cli` | pass: 17 unit, 18 integration (16 fixture tests, 2 properties at 64 cases), 14 existing verb tests |
| the two properties at 1,500 cases (`PROPTEST_CASES=1500`) | pass |
| `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check` | pass |
| `actionlint` 1.7.12 over `.github/workflows/*.yml`, with `shellcheck` 0.11.0 on the `run:` scripts | 0 findings |
| `shellcheck .github/scripts/roadmap-push.sh` | 0 findings |

How nix ran in this container: it cannot download GitHub archive tarballs,
which the flake's inputs are, so each GitHub-hosted input was cloned at its
locked revision and passed as `--override-input <name>
'git+file://<clone>?rev=<locked rev>&shallow=1'` (nixpkgs, flake-utils,
flake-utils/systems). `flake.lock` is unchanged. Nix was installed with the
single-user installer, sandboxed builds on. The same `nix flake check` passed
on `main` before any change.

actionlint and shellcheck came from the same pinned nixpkgs
(`nix shell <nixpkgs>#actionlint <nixpkgs>#shellcheck`).

**A local rehearsal of the workflows' steps.** Against a bare copy of
`roadmap-data`: two checkouts each applied a different fixture, the first
pushed, the second was rejected, merged and pushed on its second attempt, and
the union verified with two tips and listed both items. A replayed delivery
committed nothing. A fake `execution_file` went through the same `jq` the
workflow uses and was recorded with `--proposal`. That rehearsal is what
showed the bot's claim failing `verify --untrusted pricing-bot` (it was dated
behind a later delivery), which commit 3 fixes and a property now pins.

Not run: the workflows themselves on GitHub, and the Claude step, which needs
the `CLAUDE_CODE_OAUTH_TOKEN` secret. They run on the first issue after this
merges.

## Open questions

- **Branch name.** The request named `claude/issue-mirror`; this session's
  environment assigned `claude/issue-mirror-czud7t`, which is where the work
  is pushed.
- **"Not planned".** A closed issue is `Completed` as specified. GitHub's
  `state_reason: not_planned` could map to `Cancelled` instead; that is a
  one-line change if wanted.
- **Out-of-order webhooks.** The store folds in append order. GitHub does not
  promise delivery order, so a close and a reopen seconds apart could land
  reversed; the next lifecycle event settles it. No `concurrency` group is
  used, because GitHub keeps only one pending run per group and drops the
  rest.
- **Store-dependent refusals.** `/price ref` to an item not yet mirrored and
  `/price accept` before any suggestion are refused, and would apply if the
  same delivery were replayed later. The replay law is therefore stated over
  the deliveries a store accepted.
- **Build time.** Each run builds the CLI with nix from source, a few minutes
  with no binary cache. A cache (or a release binary) would make the mirror
  faster; it was left out to keep the permissions and inputs minimal.
- **Actor names.** A login becomes an actor by lowercasing, with `gh-`
  prefixed when it starts with a digit (an actor starts with a letter).
- **The viewer's reading.** The issue's URL is stored as the record's
  `source.message_id`, and the bot's rationale as its claim's note. The
  viewer being built separately should read them there, and should pass
  `pricing-bot` as untrusted.
- **Regression seeds.** `cli/tests/github.proptest-regressions` keeps the two
  cases the replay law caught during development; this repository has not
  committed such a file before.
