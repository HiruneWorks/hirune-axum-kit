# Milestones

Record durable implementation decisions and validation evidence using [TEMPLATE.md](TEMPLATE.md).
This index is updated when a milestone is complete.

- [2026-10-13 — Template foundation](2026-10-13-template-foundation.md): architecture,
  identity, durable work, operational layout, conventions, and verification scope.
- [2026-10-13 — Original files and derived search](2026-10-13-files-and-search.md):
  private storage, scoped access, bounded streaming, quotas, cleanup, search
  consistency, optional integrations, and local verification.
- [2026-10-13 — Concurrent session cleanup](2026-10-13-session-cleanup.md):
  expiry rechecks, a synchronized PostgreSQL regression, and preserved dependency
  updates from main.
- [2026-10-13 — Integrated template verification](2026-10-13-integration-verification.md):
  combined local suites, extended k6, successful implementation CI, copied-template
  validation, and Linux release-image build evidence.
- [2026-10-15 — Authentication, retention, and runtime verification](2026-10-15-security-and-verification.md):
  typed validation, administrator MFA, recovery-code replacement, bounded retention,
  mixed workloads, foreign-key lock ordering, and release-runtime acceptance.
- [2026-10-15 — Multiple providers, portable maintenance, and identity mail](2026-10-15-oidc-maintenance-and-mail.md):
  multiple OIDC providers, queue-scoped periodic maintenance, dedicated mail
  templates, account notifications, and shared error/worker verification.
- [2026-10-15 — External API keys and shared allocation](2026-10-15-external-api-access.md):
  organization-owned clients, expiring scoped keys, operator allocation, strict
  shared request accounting, usage retention, and multi-instance/runtime evidence.
- [2026-10-15 — Connected content and typed HTTP structure](2026-10-15-content-and-notifications.md):
  organization posts/comments, scoped profiles/search, transactional inbox and
  optional FCM, endpoint organization, existing/new closed-set enums, and full
  local verification including multi-instance k6 and release-runtime checks.
- [2026-10-16 — Worker dispatch](2026-10-16-worker-dispatch.md): immediate execution
  slot refill, idle-only polling, recovery progress, independent 1/2/4-pool
  regressions, and local database/process/k6 verification boundaries.
- [2026-10-17 — Technical writing](2026-10-17-technical-writing.md): practical
  ASD-STE100 clarity guidance, revised existing documentation and comments, and
  preserved technical contracts.
- [2026-10-18 — Schema consistency](2026-10-18-schema-consistency.md): automatic
  entity discovery, PostgreSQL schema comparison, string-enum CHECK validation,
  and regression tests for deliberate drift.
- [2026-10-18 — Native app authentication](2026-10-18-native-app-authentication.md):
  14/90-day default session lifetimes, native OIDC ID token login with opt-in
  client IDs, separate single-use native flows, and shared login rules.
