# Environment files

Commit only `.example/` templates. Git ignores actual `.local/`, `.dev/`, and
`.production/` settings. Image build contexts exclude these settings.
Never commit credentials or print rendered production Compose configuration.

- `just init` creates `.local/app.env` from `.example/local/app.env` with fresh keys.
- Root just commands load `.local/app.env`. Override with `just --dotenv-path <path>`.
- Production/staging operators copy `.example/production/app.env`, `migration.env`,
  and `maintenance.env` into `.<environment>/`, then supply real values through
  their secret-management workflow. API/worker settings must contain neither
  migration-role nor maintenance-role credentials.
- Each test process receives explicit settings. Tests do not read developer files.

Executables read injected environment variables only. For direct `cargo run`,
export settings yourself or use `just`. Compose passes an explicit env_file.
Deployment selectors (image, project, port) live separately under `deploy/`.

`app.env` uses the restricted runtime role, including append-only audit access.
`migration.env` supplies only migration settings and the schema-owner connection.
The deployment maintenance process reads `app.env`, then `maintenance.env`.
The latter's `DATABASE_URL` selects a restricted role that may delete expired
audit events. Provision and verify the database grants separately. An
environment-file name does not enforce database privileges.

`MAINTENANCE_INTERVAL_SECONDS` controls periodic registration (300 seconds by
default). Keep it consistent across maintenance replicas. The same process
schedules and consumes the dedicated queue; no operating-system scheduler is needed.

Set authentication limits and retention periods in `app.env`. Use the same values
on all replicas. Defaults are 10 identity attempts and 60 client requests per
minute. Retain succeeded jobs for 30 days, failed jobs for 90 days, and audit
events for 180 days. Retain API admission history for 90 days through
`API_USAGE_RETENTION_DAYS`. PostgreSQL stores external API allocations per client.
Replica environment variables do not set these allocations.
See [runtime settings](../docs/runtime.md) for names and
supported ranges. There is no password-screening provider configuration.

`OIDC_PROVIDERS` is a JSON array of provider objects, with `[]` as the disabled
default. Store it as a single quoted line in `app.env` or inject it through secret
management. Each configured provider has its own client secret. The
[runtime example](../docs/runtime.md) shows two providers; no separate per-provider
environment variable scheme is needed. Add the optional `native_client_ids` array
to a provider only when native applications sign in with its SDK.
