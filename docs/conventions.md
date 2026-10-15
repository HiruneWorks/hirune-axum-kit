# Project conventions

These are project rules, not optional suggestions. Read them with [AGENTS.md](../AGENTS.md)
and the [confirmed requirements](requirements.md). Keep rules here and behavior in
the owning contract; milestone notes record evidence and do not replace either.

## Clarity and ownership

Use names that describe purpose, even when they are long. Prefer
`revoke_account_sessions` to a vague `process` or `handle`. Keep Rust's standard
terms where they are already precise. Do not abbreviate business concepts merely
to shorten a name.

Every file, module, function, and type has one understandable responsibility.
Feature modules own their services, queries, entities, and internal types. HTTP
extraction and input/output contracts belong in `src/http/`. Group versioned
routers, handlers, and DTOs in `src/http/v1/<feature>/`; keep shared extraction,
errors, cookies, and middleware at the HTTP root. Each operation has a file named
for its purpose, containing its handler and exclusive input/output DTOs.
`routes.rs` declares URLs and methods; `contracts.rs` holds DTOs genuinely shared
by multiple operations. Module indexes declare/re-export modules and contain no
business logic or database queries. Version HTTP contracts rather
than application services. Split files when
responsibilities differ, not to meet an arbitrary line count. Crate boundaries
must clarify ownership or enforce an actual dependency boundary.

Prefer concrete, direct code. Do not add a trait, generic repository, compatibility
layer, plugin system, or configuration switch without a current need. Reuse the
owning service and policy instead of copying a rule into an endpoint or worker.
Discuss a materially different architecture before implementing it; routine
implementation choices within the agreed architecture do not need approval.

Write comments in English, briefly, only where a reason or invariant is not clear
from the code. Describe why a lock, boundary, or exception matters. Do not narrate
obvious statements. Keep public code and documentation independent of private
reference projects. This project is authored as Hirune Axum Kit, copyright 2026
Levi Laine, under Apache-2.0; do not add template-origin attribution to its prose.
Preserve any legally required third-party notices when adding third-party code.

## Persistence and API contracts

Use Axum, SeaORM, and Tokio. Prefer SeaORM for persistence and SeaORM migrations
with SeaQuery for schema operations. Raw SQL is an exception for a specifically
unsupported operation and needs a short explanation beside that operation.

Generate entity primary keys in the application with `Uuid::now_v7()`. References
to those entities are UUIDs as well. Do not introduce integer or sequence entity
IDs. Counts, ordering positions, versions, and durations may be integers. Natural
identifiers such as normalized email have their own unique constraints.
The migration helper `uuid_primary_key` declares the UUID column and primary key;
it does not generate values or restrict the UUID version.

Each migration creates one table and its indexes and constraints, or makes one
coherent subsequent schema change. Foreign keys express dependencies and migration
order. Never rewrite a migration that has been applied by a consuming project.

Services own authorization, lock order, writes, and transactions. Repository
writes require the caller's transaction. Recheck mutable authority under lock.
Commit durable jobs in the same transaction as the business change. Network calls
and password hashing stay outside database locks.

Use explicit input/output types and the common error and pagination contracts.
Keep omitted fields distinct from explicit null when an update supports both.
Do not return ORM models, silently truncate credentials, erase storage failures,
or use a broad fallback to turn an internal error into an anonymous user.

JSON and query DTOs implement `validator::Validate`; the shared extractors validate
after deserialization. Reuse pure domain validation through HTTP adapters instead
of duplicating value rules. Services repeat those checks for non-HTTP callers and
own all mutable authority checks. `ToSchema` documents a contract; it does not
validate it. Never log or return validation error parameters, which can contain
submitted secrets. See [API conventions](api.md).

## Finite value sets

Use enums for closed sets: resource/job/delivery states, authentication purposes,
membership roles, API scopes, push platforms, cursor ordering, and application
environments. Do not use unrestricted strings and repeated literal comparisons
for these internal values. Define each enum in the owning domain. Keep genuinely
open identifiers, such as caller-defined job kinds, queue names, OIDC provider
names, and opaque protocol state tokens, as validated strings.

Deserialize configuration and protocol values at the boundary. Preserve existing
wire spellings with explicit Serde mappings; HTTP enums belong to their versioned
adapter and convert into domain enums. Unknown closed values are errors, not
defaults. Map persisted closed sets through SeaORM ActiveEnum using the existing
string columns and constraints; a Rust type change does not require replacing
those columns with PostgreSQL native enums. Never rewrite applied migrations to
change a Rust representation. See [SeaORM ActiveEnum](https://www.sea-ql.org/SeaORM/docs/generate-entity/enumeration/).

## Runtime and operations

Keep `.envs/`, `compose/`, and `deploy/` responsibilities explicit. Commit only
environment examples; ignore actual environment directories. Local/container
build configuration belongs in `compose/`; deployment selectors and operator
commands belong in `deploy/`. `just` or the deployment injects settings, and
executables read that environment. Do not add implicit root-dotenv discovery or
pass schema-owner credentials to API/worker processes.

Keep runtime audit access append-only. Retention cleanup uses a separate restricted
maintenance role with audit deletion, not migration credentials. The explicit
maintenance executable schedules bounded cleanup through the shared queue; do not
start schedulers during application bootstrap or require an operating-system timer.

API and worker processes are independently replaceable instances of one modular
monolith. Shared PostgreSQL state owns sessions, MFA and OIDC flows, rate limits,
jobs, and authorization facts. In-memory caches are expendable optimizations;
they cannot be the authority for revocation or access decisions.

Use structured `tracing`, explicit settings, graceful shutdown, MJML templates,
and the shared mail transport. Do not log passwords, tokens, cookies, authorization
headers, provider responses, action URLs, or mail bodies. Keep production tracing
initialization in executable entry points so tests can create multiple apps.

Authentication changes require the [security checklist](security.md) and relevant
regression tests. Password and OIDC authentication share the same final account
and MFA checks. Optional integrations must not become default startup requirements.

## Validation and handover

Use published version tags or maintained channels for GitHub Actions references.
Do not pin actions to commit hashes. Keep Dependabot configured for Cargo, Actions,
Docker, and Compose updates. Review Rust toolchain changes across the toolchain
file, CI, Dockerfile, and documentation together.

Use `just` and the locked Cargo workspace. Choose checks by the changed contract:
unit checks, real PostgreSQL and concurrency tests, production-router API tests,
separate-process and SMTP tests, OpenAPI drift, and k6 load checks. Commands and
their prerequisites live in [development](development.md). Do not substitute a
test router or authentication bypass for the real application path.

Keep documentation, migrations, API schemas, examples, configuration, and tests
consistent with the implementation. State precisely what ran, failed, or was
skipped. Local success does not establish remote CI, deployment, provider
acceptance, or production capacity.

Preserve unrelated user changes. Commit finished work in coherent units with
concise English messages. Use descriptive branches without a `codex/` prefix.
Do not rewrite commits or run destructive Git operations without explicit user
authorization. Update `memory/` and its index at completed milestones using its
template; record durable facts, decisions, and verification limits, never secrets.
