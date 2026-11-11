# 2026-11-11 — Runtime boundaries

## Changes

- Issue [#44](https://github.com/HiruneWorks/hirune-axum-kit/issues/44) tracks session
  expiry representation, job failure correlation, and HTTP shutdown budgets.
- Draft PR [#45](https://github.com/HiruneWorks/hirune-axum-kit/pull/45) contains the
  changes in separate session, worker, and HTTP commits.
- `identity/session_expiry.rs` uses checked duration construction and timestamp
  addition. Startup validates idle and absolute lifetimes against current time.
  Session issuance and activity repeat the check with PostgreSQL time.
- Worker heartbeat and finalization failures retain job, attempt, and request UUIDs
  in their structured events. A bounded task-ID map restores claim identity for
  panics and cancellation. Normal completion and shutdown use the same handling.
- Worker errors use fixed categories. Structured events omit database error text,
  job payloads, and panic payloads. This does not change the default panic hook's
  stderr behavior. The worker does not log the same storage error a second time.
- HTTP drain is `max(30 seconds, HTTP_TIMEOUT_SECONDS + 5 seconds)`. The API passes
  this explicit duration to `runtime::serve`. The API Pod grace is 75 seconds.
  Deployment verification checks the pre-stop window and minimum API grace.
- Added 13 focused regression cases: five session boundaries, five worker log/task
  cases, and three request/drain cases. Tests were compiled, not executed locally.
- Updated identity, queue, runtime, architecture, and Kubernetes documentation.

## Decisions and constraints

- Keep the 14-day idle and 90-day absolute defaults, minimum idle lifetime, and
  absolute-versus-idle ordering. Reject representation overflow rather than add
  an arbitrary session retention policy.
- Keep PostgreSQL authoritative. Do not change schema, public HTTP DTOs, queue
  state transitions, claim/lease deadlines, retries, or handler result contracts.
- Keep one shared handler callable when moving its invocation into the spawned
  task. A synchronous handler-construction panic then retains task identity.
- Preserve the default 30-second HTTP drain. Maximum supported requests take
  60 seconds; the extra five seconds allow response completion. The API Pod budget
  covers five seconds of pre-stop, 65 seconds of drain, and five seconds of cleanup.
- Worker and maintenance Pod grace remains 45 seconds. No new environment setting,
  migration, dependency version, or release version is introduced. The jobs test
  module reuses the workspace's existing `tracing-subscriber` development dependency.
- This is a source template. Copied applications require separate adoption.

## Validation

- Installed the repository's Rust 1.98.1 with rustfmt and Clippy in a workspace-local
  tool directory. The selected environment initially had no Rust commands.
- `cargo check --locked --workspace --all-targets`: passed, including new tests.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- A read-only check of Cargo metadata and HTTP sources passed the same dependency
  and HTTP persistence predicates used by `xtask/src/checks.rs`.
- A read-only local Markdown link check passed for 59 documents, including this
  entry and the index.
- Independent Astra source review found no blocking finding.
- Initial compiler attempts stopped while a test module was still being written
  and while the new development-dependency lock entry was absent. After the files
  and lock entry were complete, locked compilation and strict Clippy passed.
- A follow-up source check found that SeaORM's disconnected sentinel panics while
  selecting a backend. The storage-error regression now uses a closed lazy
  PostgreSQL pool. This retains the real backend and produces a storage error
  without network I/O. Strict all-target Clippy for the jobs package passed after
  this fixture correction. The regression itself was not executed locally.
- Local unit, database, process, browser, and E2E suites were not executed, following
  the user's recorded preference. Do not report compiled cases as passing tests.
- Remote CI is separate and remains pending at this milestone. No image publication,
  deployment, merge, or live shutdown/provider acceptance is claimed.

## Remaining work

- Inspect PR #45 CI for the final source commit. Fix any regression before merge.
- Execute focused runtime/database acceptance when authorized. The new local checks
  do not establish production rollout behavior or provider performance.
- Keep downstream adoption separate from this template change.
