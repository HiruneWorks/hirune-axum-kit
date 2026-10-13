# Environment files

Only `.example/` templates are committed. Actual `.local/`, `.dev/`, and
`.production/` settings are ignored by Git and excluded from image build contexts.
Never commit credentials or print rendered production Compose configuration.

- `just init` creates `.local/app.env` from `.example/local/app.env` with fresh keys.
- Root just commands load `.local/app.env`. Override with `just --dotenv-path <path>`.
- Production/staging operators copy `.example/production/app.env` and
  `migration.env` into `.<environment>/`, then supply real values through their
  secret-management workflow. API/worker settings must not contain migration-role
  credentials.
- Test settings are explicit per test process; they do not read developer files.

Executables read injected environment variables only. For direct `cargo run`,
export settings yourself or use `just`. Compose passes an explicit env_file.
Deployment selectors (image, project, port) live separately under `deploy/`.
