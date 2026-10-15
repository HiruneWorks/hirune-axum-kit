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
