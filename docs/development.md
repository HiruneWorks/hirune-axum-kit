# Development

Use Rust 1.98.1, just 1.52 or newer, Docker, and Docker Compose. Commit Cargo.lock.
On Windows, Rust MSVC builds require the Visual Studio C++ build tools. All
development commands are Rust xtask or direct executable calls; no Bash-only test
wrapper is required. Start by reading [conventions](conventions.md).

GitHub Actions use version tags or maintained channels. The
[Dependabot configuration](../.github/dependabot.yml) checks Cargo, Actions,
Dockerfile images, and local Compose images weekly. Review toolchain changes
across `rust-toolchain.toml`, CI, the Dockerfile, and these instructions together.
See the [GitHub configuration reference](https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference).

| Command | Purpose |
| --- | --- |
| `just init` | Create `.envs/.local/app.env` with independent random local secrets; fail if it already exists. |
| `just infra-up` / `just infra-down` | Start/stop development PostgreSQL and Mailpit; preserve development data. |
| `just infra-up-files` | Start PostgreSQL, Mailpit, SeaweedFS, and Meilisearch with local-only credentials. |
| `just migrate` | Apply SeaORM migrations under a PostgreSQL advisory transaction lock. |
| `just api` / `just worker` | Start the actual API/worker executable. |
| `just manage cleanup` | Remove bounded batches of expired authentication rows and old successful jobs, and reconcile file cleanup. |
| `just manage retry-job <uuid>` | Retry a retained failed job without erasing its attempt history. |
| `just manage initialize-integrations` | Explicitly create the configured private bucket and file search index/settings; safe to repeat. |
| `just manage repair-search` | Queue a bounded batch of current file metadata for index reconciliation. |
| `just fmt` | Format the workspace. |
| `just check` | Formatting, architecture boundaries, local documentation links, Clippy with warnings denied, unit/doc tests, OpenAPI drift. |
| `just test-db` | Real PostgreSQL identity and queue tests, including transaction and concurrency failures. |
| `just test-api` | Production-router API tests and local signed OIDC provider tests. |
| `just test-process` | Separate API/worker processes, real HTTP, Mailpit SMTP, worker replacement, shared session revocation, and log secrecy. |
| `just test-files` | Real SeaweedFS/Meilisearch, original-byte preservation, range downloads, tenant access, quota races, cancellation, and late-upload deletion. |
| `just test-deployment` | Validate local/deployment Compose and separation of runtime and migration credentials using temporary example files. |
| `just load-test` | Docker k6 smoke thresholds against a real API process. |
| `just openapi` | Regenerate the committed OpenAPI document. |
| `just template-check` | Copy/rename the source into a temporary directory and run its workspace and identity/database tests. |

## Database isolation

Integration commands create a unique Compose project with dynamic local ports,
then remove only that project's containers and volumes. Each test creates its own
UUID-named database and applies migrations. Test cleanup drops only those generated
databases. Plain `cargo test --workspace` leaves infrastructure tests ignored;
the explicit integration commands select them with `--ignored`.

For an existing local disposable PostgreSQL server, set `TEST_DATABASE_URL`.
The role must be allowed to create and drop test databases. Non-loopback database
hosts are rejected. Process/load tests additionally require `TEST_SMTP_PORT` and
`TEST_MAILPIT_URL`. Do not point test administration at a production database.
File tests additionally require loopback `TEST_S3_ENDPOINT` and
`TEST_MEILISEARCH_URL` using the local Compose credentials. `just test-files`
starts these services automatically in its disposable Compose project.
Test databases from a panic may remain in external infrastructure; inspect their
generated `kit_test_` names before explicit cleanup.

## Load evidence

The default k6 profile runs two virtual users for ten seconds against readiness,
authenticated account reads, and organization pagination. It fails on any failed
business check, an HTTP failure rate of 1% or higher, or p95 at/above two seconds.
`K6_PROFILE=extended` uses twenty users for two minutes. Set the environment in
your shell before `just load-test`. The workflow dispatch input selects that
profile in CI. Thresholds are regression gates, not a production capacity promise.
Benchmark production builds, realistic data, distinct user populations, deployed
network paths, and intended concurrency before a capacity decision.

## Editing and diagnostics

Read a feature's behavior contract before changing service rules. Add tests at the
boundary affected: pure policy/input logic, database atomicity, HTTP extraction,
or actual process/external effects. Avoid tests that merely repeat a field setter.
Use axum-test on `build_router`; do not create alternate test-only routes. Tests
may create persistence fixtures, but runtime requests use real authentication.

Do not suppress internal errors or broadly disable warnings. An infrastructure
failure is not a passing test. Record actual commands and limitations in a
milestone, separately from CI, release, deployment, and live provider acceptance.
OpenAPI consumers regenerate through their own pipelines; never hand-edit a
generated client. Keep release metadata consistent when adding a release pipeline.
