# Runtime and deployment

`api`, `worker`, `migration`, and `manage` are separate executables from one source
workspace. The API owns HTTP lifecycle; the worker owns bounded job execution;
migration and management run explicitly. `bootstrap(settings)` assembles shared
dependencies, `build_router` constructs the production router, and `serve` accepts
an existing listener and shutdown token. Tests can create multiple independent
applications without modifying global environment or initializing global tracing.

## Shared authority

PostgreSQL owns sessions, security versions, challenges, OAuth flows, rate limits,
organizations, job leases, and delivery state. API instances require no sticky
routing. Worker concurrency can be increased with more processes; database locks
and fencing remain authoritative across them. In-process semaphores only bound
local CPU/request capacity. Redis, a broker, and provider credentials are not
baseline startup dependencies.

Database connections have bounded connection/acquisition times and configured
pool size. Budget the sum across API and worker replicas against PostgreSQL's
connection limit. Run migrations first. Bootstrap checks migration history using
a read-only query and refuses an outdated schema; it does not apply DDL.
Use a schema-owner migration role and a runtime role restricted to the required
tables, sequences if introduced by dependencies, and migration-history SELECT.
Development Compose uses a privileged local role solely for convenience/testing.

## Configuration

`.envs/.example/local/app.env` lists local settings. `just init` generates keys; never reuse public
test fixtures. `Settings::from_values` validates explicit configuration; only
executables read injected environment. Root just recipes load `.envs/.local/app.env`,
and deployment Compose uses explicit env_file paths. Production requires HTTPS application,
origin, and provider URLs plus STARTTLS SMTP. Cookie security follows APP_ENV.
Secrets use redacted Debug representations. Do not log complete settings.

Configure a provider with OIDC_ISSUER, OIDC_CLIENT_ID, OIDC_CLIENT_SECRET,
OIDC_REDIRECT_URL, and OIDC_PROVIDER. Without them, the app starts without OIDC.
Configured providers must be reachable for initial discovery. SENTRY_DSN is optional;
SENTRY_SAMPLE_RATE controls performance sampling when enabled. The SMTP server
need not be reachable at API startup; delivery failure follows the queue contract.

Behind a proxy, configure exact ALLOWED_ORIGINS and the explicit TRUSTED_PROXY_IPS
contract in [security](security.md). Protect the backend from direct untrusted
network access. Do not enable forwarding-header trust globally.

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
Schedule `manage cleanup` outside application startup. See [jobs and mail](behavior/jobs-and-mail.md).

## Container

Build with `docker build -f compose/production/app/Dockerfile -t hirune-kit .`. The default entrypoint is the API;
select `--entrypoint worker`, `--entrypoint migration`, or `--entrypoint manage`
for other roles. The image runs as a non-root user and contains no source keys.
Supply environment/secrets at deployment, set BIND_ADDRESS=0.0.0.0:3000 for the
API container, run migrations once before rollout, and arrange health probes and
termination grace periods. The development Compose file does not deploy this image.
See [environment layout](../.envs/README.md), [container files](../compose/README.md),
and [deployment commands](../deploy/README.md) for the canonical paths.
