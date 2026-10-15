# Container configuration

The optional `files` and `search` profiles start SeaweedFS 4.48 and Meilisearch
1.55.0. `just infra-up-files` enables both; ordinary `just infra-up` keeps them
disabled. Gateway ports bind to loopback, and the filer/master are not published.
`seaweedfs/s3.local.json` contains public local development credentials. It is not
a production credential file. The S3 gateway requires authentication.

`compose.local.yml` supplies PostgreSQL and Mailpit for local development. Verification
uses the same services with unique project names, dynamic ports, and disposable
volumes. Development volumes are preserved by `just infra-down`.

`production/app/Dockerfile` builds API, worker, migration, and management binaries
from the repository-root build context. It excludes environment secrets and runs
as an unprivileged user. See [deployment](../deploy/README.md) for runtime wiring.

`verification/` contains the disposable production-mode acceptance fixture:
private TLS certificates, an HTTPS proxy, STARTTLS Mailpit, restricted PostgreSQL
roles, and database restore checks against the release image. Run it through
`just test-production`; see [verification scope](../docs/development.md#production-fixture-scope).
Its scripts provision test infrastructure and are not deployment scripts.
