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
| `just maintenance` | Start the portable periodic scheduler/worker; production supplies separate maintenance-role credentials. |
| `just manage cleanup` | Remove bounded expired authentication/audit/finished-job batches and reconcile files; production uses the maintenance role through `deploy/justfile cleanup`. |
| `just manage set-administrator <uuid> <true\|false>` | Operator-only role change; promotion requires enrolled MFA and changes revoke existing credentials. |
| `just manage retry-job <uuid>` | Retry a retained failed job without erasing its attempt history. |
| `just manage initialize-integrations` | Explicitly create the configured private bucket and file search index/settings; safe to repeat. |
| `just manage repair-search` | Queue a bounded batch of current file metadata for index reconciliation. |
| `just fmt` | Format the workspace. |
| `just check` | Formatting, architecture boundaries, local documentation links, Clippy with warnings denied, unit/doc tests, OpenAPI drift. |
| `just test-db` | Real PostgreSQL identity and queue tests, including transaction and concurrency failures. |
| `just test-api` | Production-router API tests and local signed OIDC provider tests. |
| `just test-process` | Separate API/worker/maintenance processes, four mail purposes through Mailpit, worker replacement, shared revocation, and periodic cleanup deduplication. |
| `just test-files` | Real SeaweedFS/Meilisearch, original-byte preservation, range downloads, tenant access, quota races, cancellation, and late-upload deletion. |
| `just test-deployment` | Validate local/deployment Compose and separation of runtime, maintenance, and migration credentials using temporary example files. |
| `just test-production` | Build and run the release container with local TLS/STARTTLS fixtures, restricted PostgreSQL roles, and backup/restore checks. |
| `just load-test` | Docker k6 mixed workload against two API processes, a worker, SeaweedFS, and Meilisearch. |
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

The default k6 profile uses two virtual users for ten seconds, each with its own
account and organizations. It exercises scoped reads and pagination, optimistic
updates, cross-organization denial, cross-instance session rotation/revocation,
and original-file upload/download/search/deletion. Search visibility waits are
bounded. Expected rejection probes have explicit allowed statuses; unexpected
HTTP failures and failed business checks fail the run. It also rejects p95
latency at/above two seconds. Thresholds require `http_req_failed: rate==0`,
`checks: rate==1`, and `workflow_failures: count==0`; every virtual user must
complete a full workflow and revoke its separate session once. Search visibility
must remain below ten seconds for each workflow, with a matching p95 threshold.
After k6 exits, originals must reach the deleted state and pending work must drain.
Any failed background attempt also fails the check, even if a later retry succeeds.
`K6_PROFILE=extended` uses twenty users for two minutes. Set the environment in
your shell before `just load-test`. The workflow dispatch input selects that
profile in CI. Thresholds are regression gates, not a production capacity promise.
Benchmark production builds, realistic data, distinct user populations, deployed
network paths, and intended concurrency before a capacity decision.

## Production fixture scope

`just test-production` builds the canonical release image and runs it with
`APP_ENV=production` inside a uniquely named local Compose project. Set
`PRODUCTION_TEST_IMAGE` only when deliberately verifying an existing local image;
that supplied image is preserved during cleanup. The fixture uses an internal
Docker network, an ephemeral private CA, an HTTPS proxy, and Mailpit requiring
STARTTLS. It checks secure cookies, origins, proxy header replacement, and actual
Rust-worker mail delivery, including rejection of an untrusted SMTP certificate
and a certificate whose hostname does not match. Runtime containers use a
non-root user and read-only filesystem.

The fixture provisions separate migration, runtime, and maintenance PostgreSQL
roles and exercises allowed and denied operations, including scheduled maintenance
with restricted credentials. It stops API, worker, and maintenance processes,
backs up PostgreSQL, recreates the database, restores the dump, and signs in with
the restored password and encrypted TOTP state using retained encryption keys.
Only fixture containers, volumes, and generated images are removed. These checks
cover local release-image behavior; they do not publish or deploy the image,
verify a real SMTP/OIDC provider, or test S3 original-file backup and restoration.
Record actual execution results in [milestones](../memory/README.md); the presence
of this command or its CI step is not evidence that it passed.

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
