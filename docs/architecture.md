# Architecture

Hirune Axum Kit is a source template for one service. Copy the repository, change
the application name and settings, and own the result. Internal crates are not
independently published products. Template updates do not migrate copied apps.

## Execution and ownership

This is a modular monolith with separately scalable API and worker processes.
PostgreSQL owns accounts, authentication state, authorization facts, deliveries,
and jobs. Neither process requires sticky routing or in-memory authority.
Migration and management commands are explicit one-off processes.

| Location | Owns | May depend on |
| --- | --- | --- |
| `src/http/` | HTTP extraction, DTOs, middleware, error mapping, OpenAPI | application services |
| `src/bootstrap.rs` | connection and service assembly | application, jobs, mail |
| `src/bin/` | environment, process tracing, executable lifecycle | bootstrap and runtime |
| `crates/application/src/identity/` | accounts, credentials, sessions, authentication flows | auth-core, jobs, mail, SeaORM |
| `crates/application/src/organizations/` | minimal organizations and scoped memberships | authorization, SeaORM |
| `crates/application/src/authorization/` | validated Cedar policies and decisions | Cedar |
| `crates/jobs/` | durable registration, leases, retries, attempt history | SeaORM, Tokio |
| `crates/mail/` | MJML/Minijinja rendering and SMTP | lettre, mrml, minijinja |
| `crates/auth-core/` | small cryptographic wrappers | established crypto libraries |
| `crates/migration/` | schema history | SeaORM migrations |
| `crates/test-support/` | isolated databases and fixtures | application, migration |
| `xtask/` | repeatable development and verification commands | tooling only |
| `.envs/` | tracked examples and ignored per-environment secrets | configuration only |
| `compose/` | local service composition and production image build | container tooling |
| `deploy/` | environment selectors and runtime/operator wiring | container tooling |

The application crate has no Axum dependency. Its feature modules contain their
own service, repository, model, and types. HTTP DTOs stay in the HTTP adapter.
Do not create a crate for every module or a trait for every concrete service.
Introduce a boundary when it makes ownership, dependencies, or testing clearer.

Crate boundaries enforce dependency direction, not a list of technical layers.
Keeping each feature's service, repository, entities, and internal types together
makes a business change local. Jobs and mail have independent execution and
failure contracts; auth-core has no database or async runtime dependency.
Migration history and development/test tools do not belong in request handling.
API, worker, and management are binaries of the same assembled application;
separate process deployment does not require separate service crates.

`bootstrap(settings)` prepares dependencies. `build_router(app, http_settings)`
only constructs the production router. `serve(listener, router, shutdown)` owns
listening and shutdown. Tests use these same functions; router construction must
not start a listener, consumer, scheduler, or detached task.

## Extending the template

Add a feature with explicit service operations and feature-owned persistence.
Expose it through thin HTTP handlers and typed input/output contracts. Add the
operation's behavior, transaction boundary, authorization, pagination, and tests
together. Workers call those same operations when the business behavior is shared.

The initial worker backend is PostgreSQL. Changing a backend is a deliberate
adapter and transaction decision: a broker needs an outbox when a domain write
must commit with publication. A common function signature cannot create a
distributed transaction.

Search, object storage, boards, comments, SSE, campaign mail, and workflow engines
are outside the baseline. They can be added by the project that needs them.

## Design references

- [Axum state and Tower integration](https://docs.rs/axum/latest/axum/)
- [SeaORM transactions](https://www.sea-ql.org/SeaORM/docs/advanced-query/transaction/)
- [Django database transactions](https://docs.djangoproject.com/en/6.0/topics/db/transactions/): explicit transaction boundaries and durable side-effect handling inform the conventions.
- [Django sessions](https://docs.djangoproject.com/en/6.0/topics/http/sessions/): shared server-side authority and credential rotation inform session behavior.

These are design references. The implementation remains ordinary Rust and Axum.
