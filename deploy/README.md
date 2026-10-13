# Deployment

This directory owns runtime wiring and operator commands. Image build files live
in `compose/`; secret settings live in ignored `.envs/.<environment>/` directories.
The initial deployment uses an externally provisioned PostgreSQL database and
SMTP server. Database backups, TLS termination, and provider configuration are
deployment responsibilities.

1. Build and publish an application image with `compose/production/app/Dockerfile`
   and the repository root as context. Publishing is an explicit operator action.
2. Copy `production.env.example` to `production.env`; set an immutable image tag
   or digest, unique COMPOSE_PROJECT_NAME, and API_PORT.
3. Copy the production examples under `.envs/.example/production/` into
   `.envs/.production/`. Set HTTPS origins, runtime database credentials, SMTP, and
   independently generated encryption/rate-limit keys. Put the schema-owner URL
   only in `migration.env`.
4. From this directory run `just check production`, then `just up production`.
   The latter runs migrations before starting/replacing API and worker.
5. Configure the host TLS proxy and readiness probe, verify mail/provider behavior,
   and arrange `just manage production cleanup` through the deployment scheduler.

A staging environment uses its own selector, project name, ports, database, and
`.envs/.<environment>/` directory. Production APP_ENV also applies to HTTPS staging.
Do not share authority tables or secret keys between unrelated environments.
The API binds to host loopback; configure a host reverse proxy and explicit proxy
trust before accepting public traffic. Workers can scale independently. Multiple
API replicas need deployment-level routing/port allocation; this single-host
example is not a load-balancer implementation.

`just down` preserves database data because this Compose project does not own the
database. Cleanup and retry operations are explicit. No deployment, image push,
external messages, or scheduler changes occur when running local checks.
