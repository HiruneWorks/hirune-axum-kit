# Hirune Axum Kit

A Rust source template for a maintainable Axum service. It uses SeaORM,
PostgreSQL, Tokio, structured tracing, Cedar authorization, and MJML email.
API and worker processes scale independently while sharing durable authority.

## Start locally

Install Rust 1.98.1, just, and Docker with Compose. Then run:

```sh
just init
just infra-up
just migrate
just api
```

Run `just worker` in another terminal. The API listens on localhost:3000,
Mailpit on localhost:8029, and OpenAPI is served at `/openapi.json`.
`just init` creates independent local keys and never overwrites an existing `.envs/.local/app.env`.
Email action links expect a consuming frontend to read the fragment and submit
the token to the documented confirmation endpoint; this template has no frontend.

## Verify

```sh
just check
just test-db
just test-api
just test-process
just test-files
just test-deployment
just load-test
just template-check
```

Database and process commands create disposable, uniquely named Compose projects
and isolated test databases. See [development](docs/development.md) for external
local infrastructure, test prerequisites, and load-test profiles. CI uses the same
commands; checked-in workflows are not evidence of a successful remote run.

## Optional file storage and search

Run `just infra-up-files`, enable the commented S3/Meilisearch settings in
`.envs/.local/app.env`, and run `just manage initialize-integrations` after
migrations. The API stores original bytes in the private S3 bucket. PostgreSQL
controls attachment access; Meilisearch indexes filenames only. Run the worker
for deletion and indexing, and schedule bounded `manage cleanup` and
`manage repair-search` operations in deployment. See the
[file contract](docs/behavior/files-and-search.md) for limits and lifecycle.

## Own the source

Copy the repository and change the root Cargo package name, README/NOTICE,
Compose project name, and application settings for your project. Keep the library
name `kit` unless you also change Rust imports. Regenerate Cargo.lock and OpenAPI,
then run the checks. Internal crates are source organization, not separate products
or an automatic template-upgrade mechanism.

- [Architecture](docs/architecture.md) and [runtime](docs/runtime.md)
- [Environment layout](.envs/README.md), [container files](compose/README.md), and [deployment](deploy/README.md)
- [Confirmed requirements](docs/requirements.md) and [code conventions](docs/conventions.md)
- [API conventions](docs/api.md) and [generated OpenAPI](docs/openapi.json)
- [Behavior contracts](docs/behavior/README.md) and [transaction boundaries](docs/transaction-boundaries.md)
- [Security controls](docs/security.md) and [regression inventory](docs/regressions.md)
- [Agent instructions](AGENTS.md) and [milestone evidence](memory/README.md)

Copyright 2026 Levi Laine. Licensed under [Apache-2.0](LICENSE).
