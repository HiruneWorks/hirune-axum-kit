# Runtime and deployment

`api`, `worker`, `maintenance`, `migration`, and `manage` are separate executables
from one source workspace. The API owns HTTP lifecycle; the worker owns bounded job execution;
migration and management run explicitly. `bootstrap(settings)` assembles shared
dependencies, `build_router` constructs the production router, and `serve` accepts
an existing listener and shutdown token. Tests can create multiple independent
applications without modifying global environment or initializing global tracing.

## Shared authority

PostgreSQL owns sessions, security versions, challenges, OAuth flows, rate limits,
organizations, external API clients/keys/usage budgets, job leases, and delivery
state. API instances require no sticky routing. Worker concurrency can be
increased with more processes; database locks
and fencing remain authoritative across them. In-process semaphores only bound
local CPU/request capacity. Redis, a broker, and provider credentials are not
baseline startup dependencies.

Database connections have bounded connection/acquisition times and configured
pool size. Budget the sum across API and worker replicas against PostgreSQL's
connection limit. Run migrations first. Bootstrap checks migration history using
a read-only query and refuses an outdated schema; it does not apply DDL.
Use a schema-owner migration role and a runtime role restricted to the required
tables, sequences if introduced by dependencies, and migration-history SELECT.
Runtime audit and API-usage access is append-only: it must not update or delete
audit_events or api_usage. A separate maintenance role adds deletion on those
tables for retention, with no UPDATE or TRUNCATE privilege, while
retaining the other permissions needed for cleanup; it must not own the schema
or have DDL privileges. Deployment Compose injects this role only into its explicit
maintenance service. Provision and verify these database grants in the deployment;
separate environment files alone do not enforce permissions. Development Compose
uses a privileged local role solely for convenience/testing.

## Configuration

`.envs/.example/local/app.env` lists local settings. `just init` generates keys; never reuse public
test fixtures. `Settings::from_values` validates explicit configuration; only
executables read injected environment. Root just recipes load `.envs/.local/app.env`,
and deployment Compose uses explicit env_file paths. Production requires HTTPS application,
origin, and provider URLs plus STARTTLS SMTP. Cookie security follows APP_ENV.
Secrets use redacted Debug representations. Do not log complete settings.

Configure any number of named providers with the `OIDC_PROVIDERS` JSON array.
Each entry requires `name`, `issuer`, `client_id`, `client_secret`, and `redirect_url`:

```json
[
  {
    "name": "google",
    "issuer": "https://accounts.google.com",
    "client_id": "replace-with-google-client-id",
    "client_secret": "replace-with-google-client-secret",
    "redirect_url": "https://app.example.com/api/v1/auth/oidc/google/callback"
  },
  {
    "name": "company",
    "issuer": "https://identity.example.com",
    "client_id": "replace-with-company-client-id",
    "client_secret": "replace-with-company-client-secret",
    "redirect_url": "https://app.example.com/api/v1/auth/oidc/company/callback"
  }
]
```

In `app.env`, encode this as one line inside single quotes:
`OIDC_PROVIDERS='[{...},{...}]'`, replacing the placeholders with complete objects.
Keep the JSON and its client secrets out of logs. Omitted, blank, or `[]` disables
OIDC. Provider names are unique, contain ASCII letters, digits, or hyphens, and
are at most 64 bytes. Empty credentials, unknown fields, duplicate names, and
invalid URLs fail configuration; production issuer and callback URLs require
HTTPS. The old individual `OIDC_*` provider settings are rejected rather than
silently ignored. Keep provider definitions and compatible secret keys consistent
across API replicas. Routes select the provider by its `name`; identity ownership
uses the verified issuer and subject. This configuration supports OIDC providers;
OAuth-only integrations need their own protocol adapter.

Configured providers must be reachable for initial discovery. SENTRY_DSN is optional;
SENTRY_SAMPLE_RATE controls performance sampling when enabled. The SMTP server
need not be reachable at API startup; delivery failure follows the queue contract.

Authentication budgets are `AUTH_IDENTITY_LIMIT_PER_MINUTE` (default 10 per
operation/identity) and `AUTH_CLIENT_LIMIT_PER_MINUTE` (default 60 shared across
authentication routes per client). Both accept 1 through 100000. Retention settings
are `SUCCEEDED_JOB_RETENTION_DAYS` (30), `FAILED_JOB_RETENTION_DAYS` (90), and
`AUDIT_RETENTION_DAYS` (180), `API_USAGE_RETENTION_DAYS` (90), and
`NOTIFICATION_RETENTION_DAYS` (90), each from 1
through 36500 days. API, worker, and
management settings should agree across replicas; changing a limit does not reset
shared counters.

External API request allocation is stored per client in PostgreSQL rather than
replica environment variables. New clients receive zero allocation; a trusted
operator runs `manage set-api-client-quota <uuid> <per-minute> <per-month>`.
The quotas use fixed UTC minutes and UTC calendar months. See
[API access](behavior/api-access.md) for accounting and extension boundaries.

`MAINTENANCE_INTERVAL_SECONDS` defaults to 300 and supports 1 through 86400.
Run `maintenance` with the restricted maintenance database role. It schedules one
durable job per database-time interval bucket in the `maintenance` queue and
consumes that queue with the common job runtime. Normal workers consume only the
`application` queue. Multiple maintenance replicas use the same interval and
deduplicate registration in PostgreSQL. Each process runs one maintenance job at
a time; retries or adjacent buckets may overlap across replicas. Cleanup and
search repair tolerate repeated execution. Missing buckets during a full outage
are not backfilled; existing queued jobs resume and a current-bucket job is
registered on restart. Retention and explicit manual retry cover both queues.

Behind a proxy, configure exact ALLOWED_ORIGINS and the explicit TRUSTED_PROXY_IPS
contract in [security](security.md). Protect the backend from direct untrusted
network access. Do not enable forwarding-header trust globally.

## Optional search and push

The shared `MEILISEARCH_URL` and `MEILISEARCH_API_KEY` enable derived filename,
post, and profile search. Each has a distinct index: `MEILISEARCH_INDEX` defaults
to files, `MEILISEARCH_POSTS_INDEX` to posts, and `MEILISEARCH_PROFILES_INDEX`
to profiles. Duplicate names fail configuration. Run
`manage initialize-integrations` after migrations; startup never performs
integration provisioning. `manage repair-search` and scheduled maintenance share
one explicit coordinator and enqueue at most 100 rows per configured domain per
run, prioritizing never/least-reconciled rows. Repeated repair eventually visits
the current dataset; there is no implicit full rebuild at startup.

`FCM_ENABLED=false` is the default. The inbox and comment transactions work
without Google credentials. Enabling FCM requires `FCM_PROJECT_ID`; workers use
Application Default Credentials lazily for the Firebase Messaging OAuth scope.
Use workload identity in hosted deployments, or supply
`GOOGLE_APPLICATION_CREDENTIALS` and mount its service-account file read-only.
That path is a Google runtime setting, not an application secret copied into
the repository or image. A valid project setting alone does not verify provider
permissions or device receipt. See [notifications and push](behavior/notifications-and-push.md).

## Telemetry and shutdown

Executables initialize structured JSON tracing once. HTTP spans include a generated
request UUID, matched route pattern, method, status, and duration. Services skip
arguments in instrumentation; job spans add job/attempt IDs and the originating
request ID. SMTP acceptance includes delivery ID. Correlation crosses the database
queue through request ID; this is not W3C traceparent propagation.

The optional Sentry layer records owned application errors and performance spans,
with request-local hubs. Provider/library events are filtered, request/user payloads
are removed, and raw URLs, headers, bodies, or SQL parameters are not captured by
HTTP instrumentation. Future fields need explicit redaction review. Do not enable
verbose third-party logging for production credentials.

SIGTERM/Ctrl-C changes readiness and begins bounded shutdown. Liveness reports
process availability; readiness also requires the database and an accepting API.
The API drains for at most thirty seconds. Workers stop claiming and drain for
WORKER_DRAIN_SECONDS, then cancel unfinished tasks; leases recover them later.
Run the explicit `maintenance` process for periodic cleanup and search repair,
using the restricted maintenance role. Deployment starts it alongside API and
worker; no host cron or systemd timer is required. `manage cleanup` remains an
explicit one-off command using the same cleanup implementation.
See [jobs and mail](behavior/jobs-and-mail.md).

## Container

Build with `docker build -f compose/production/app/Dockerfile -t hirune-kit .`. The default entrypoint is the API;
select `--entrypoint worker`, `--entrypoint migration`, or `--entrypoint manage`
for other roles. The image runs as a non-root user and contains no source keys.
Supply environment/secrets at deployment, set BIND_ADDRESS=0.0.0.0:3000 for the
API container, run migrations once before rollout, and arrange health probes and
termination grace periods. The development Compose file does not deploy this image.
See [environment layout](../.envs/README.md), [container files](../compose/README.md),
and [deployment commands](../deploy/README.md) for the canonical paths.
