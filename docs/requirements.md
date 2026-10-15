# Confirmed requirements

This document records the owner's explicit constraints and accepted choices.
Reference material informs design; it does not override these requirements.
Changing a requirement materially requires a new decision, followed by an update
here and in the relevant behavior contract. This is a requirements record, not a
claim that every verification step has already passed.

| Area | Required direction |
| --- | --- |
| Deliverable | A fresh, self-contained Rust source template, owned by each project that copies it. No backward compatibility with reference projects is required. |
| Ownership | Hirune Axum Kit; copyright 2026 Levi Laine; Apache-2.0. No template-origin branding in project prose. |
| Core stack | Axum, SeaORM, Tokio, structured tracing, MJML email templates, and justfile commands. |
| Operational layout | `.envs/` owns environment examples and ignored secrets; `compose/` owns container/local configuration; `deploy/` owns deployment wiring and commands. |
| Design | Independently reasoned crate and file boundaries. References demonstrate useful behavior, not a prescribed crate diagram. |
| Code quality | Clarity and maintainability first; explicit descriptive names, even when longer; concise English comments only where needed. |
| Persistence | Prefer SeaORM and SeaQuery, including migrations. Application-generated UUIDv7 entity primary keys and UUID references. |
| Migrations | One table with its own indexes and constraints per baseline migration. Subsequent migrations have a clear schema responsibility. |
| Scaling | Replaceable stateless API instances; shared state outside processes; independently scalable workers. |
| Authentication | OWASP-informed controls, common password/OIDC account and MFA gate, server-side revocation, durable shared authentication state. |
| Privileged authentication | Global administrators must have confirmed TOTP enrollment. Role changes use an operator command, invalidate existing credentials, and are audited. Ordinary accounts may choose MFA. |
| Password screening | No external screening service. A local common/breached-password blocklist is deferred; do not add one without a new decision. |
| Retention | Configurable defaults: succeeded jobs 30 days, failed jobs 90 days, audit events 180 days. Cleanup is bounded and explicitly scheduled; runtime audit access stays append-only. |
| Device policy | New-device email verification is excluded from the default template. Use MFA for the additional authentication factor. |
| Testing | Meaningful unit, PostgreSQL, API, process/SMTP, and k6 load coverage; reproducible GitHub CI commands. |
| Files | Original bytes in a separate private S3-compatible SeaweedFS store; PostgreSQL owns metadata, permissions, quota reservations, and lifecycle. No default content sanitization, antivirus, image reencoding, video processing, or media processor. |
| Search | Meilisearch is a derived index behind the authenticated API; current PostgreSQL state and permissions decide which results may be returned. |
| Dependency maintenance | Use version tags or maintained channels for GitHub Actions, with Dependabot updates; do not pin action references to commit hashes. |
| Agent guidance | Concise mandatory instructions in AGENTS.md, detailed conventions and contracts in docs, milestone evidence in memory. |
| Git | Coherent local commits with concise English messages; no `codex/` branch prefix; preserve unrelated work. |

## Accepted architecture

The initial architecture is a modular monolith, with separate API and worker
executables using the same application services and PostgreSQL database. It is
not a fleet of microservices. Shared storage makes instances replaceable without
sticky routing. The source layout follows feature ownership and real dependency
boundaries; it is not a framework or a generic abstraction over every backend.

Redis is not a baseline dependency. PostgreSQL owns sessions, challenges, OIDC
flows, rate limits, and jobs. A future cache must have explicit consistency and
failure semantics and must never authorize a revoked session. Provider public
key caching is separate from account and session authority.

Password signup requires email verification. MFA is optional for ordinary accounts
and mandatory for global administrators. Once enabled it applies to every supported
primary login method. Recovery-code regeneration requires recent authentication
and a fresh factor proof, replaces all codes, and invalidates existing credentials.
New-device email verification is intentionally excluded. OIDC linking requires an
authenticated account; matching email addresses alone do not merge accounts.

## Scope boundaries

The baseline demonstrates identity, a small organization domain, scoped
authorization and pagination, durable background work, and transactional mail.
The file example demonstrates organization attachments and filename search.
Conversation-specific attachment permissions belong to the owning conversation
service in a consuming project. Arbitrary file contents are not parsed or executed;
`ready` describes completed storage, not a malware or content-safety verdict.
Storage and search are separately optional integrations. Public profiles, social
content, notification inboxes, campaign email, and application-specific
permissions are project extensions.
The [regression inventory](regressions.md) records retained security behavior and
intentional exclusions from the reference test coverage.

Use [architecture](architecture.md) for ownership, [conventions](conventions.md)
for code rules, and [behavior contracts](behavior/README.md) for public behavior.
Use [milestones](../memory/README.md) for actual validation status and remaining
work. Do not treat a requirement, a planned CI job, or a documentation statement
as evidence that a runtime check passed.
