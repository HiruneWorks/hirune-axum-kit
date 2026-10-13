# Container configuration

`compose.local.yml` supplies PostgreSQL and Mailpit for local development. Verification
uses the same services with unique project names, dynamic ports, and disposable
volumes. Development volumes are preserved by `just infra-down`.

`production/app/Dockerfile` builds API, worker, migration, and management binaries
from the repository-root build context. It excludes environment secrets and runs
as an unprivileged user. See [deployment](../deploy/README.md) for runtime wiring.
