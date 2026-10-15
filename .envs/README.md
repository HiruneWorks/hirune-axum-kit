# Environment files

Only `.example/` templates are committed. Actual `.local/`, `.dev/`, and
`.production/` settings are ignored by Git and excluded from image build contexts.
Never commit credentials or print rendered production Compose configuration.

- `just init` creates `.local/app.env` from `.example/local/app.env` with fresh keys.
- Root just commands load `.local/app.env`. Override with `just --dotenv-path <path>`.
- Production/staging operators copy `.example/production/app.env`, `migration.env`,
  and `maintenance.env` into `.<environment>/`, then supply real values through
  their secret-management workflow. API/worker settings must contain neither
  migration-role nor maintenance-role credentials.
- Test settings are explicit per test process; they do not read developer files.

Executables read injected environment variables only. For direct `cargo run`,
export settings yourself or use `just`. Compose passes an explicit env_file.
Deployment selectors (image, project, port) live separately under `deploy/`.

`app.env` uses the restricted runtime role, including append-only audit access.
`migration.env` supplies only migration settings and the schema-owner connection.
The deployment maintenance process reads `app.env` followed by `maintenance.env`, whose
`DATABASE_URL` overrides the runtime connection with a restricted role that may
delete expired audit events. Provision and verify the actual grants separately;
an environment-file name does not enforce database privileges.

`MAINTENANCE_INTERVAL_SECONDS` controls periodic registration (300 seconds by
default). Keep it consistent across maintenance replicas. The same process
schedules and consumes the dedicated queue; no operating-system scheduler is needed.

Authentication limits and retention periods belong in `app.env` and should agree
across replicas. Defaults are 10 identity attempts and 60 client requests per
minute, with succeeded jobs retained for 30 days, failed jobs for 90 days, and
audit events for 180 days, and API admission history for 90 days through
`API_USAGE_RETENTION_DAYS`. External API allocations are stored per client in
PostgreSQL, rather than in replica environment variables.
See [runtime settings](../docs/runtime.md) for names and
supported ranges. There is no password-screening provider configuration.

`OIDC_PROVIDERS` is a JSON array of provider objects, with `[]` as the disabled
default. Store it as a single quoted line in `app.env` or inject it through secret
management. Each configured provider has its own client secret. The
[runtime example](../docs/runtime.md) shows two providers; no separate per-provider
environment variable scheme is needed.
