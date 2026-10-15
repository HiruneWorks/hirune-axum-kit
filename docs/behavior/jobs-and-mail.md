# Jobs and mail

Application services register a job using their database transaction. A rollback
removes the domain change and job together. Job payloads are typed at the owning
handler and bounded at 16 KiB; authentication mail jobs carry only a delivery UUID.
Do not put credentials or bodies in queue metadata or tracing fields.

Workers claim ready rows with PostgreSQL row locks and SKIP LOCKED. Every attempt
has a new UUIDv7 claim token and a history row. Heartbeat and completion require
the current token, running state, and a live lease. A stale worker cannot overwrite
a replacement's result. Database time governs lease authority; a corresponding
Tokio monotonic deadline bounds the complete handler/heartbeat operation.

Successful work becomes succeeded. Retryable failure becomes pending with
exponential delay, bounded by the attempt budget. Permanent failure or exhausted
attempts become retained failed jobs. An expired claim closes its attempt history
and either schedules a delayed retry or retains failure. The default budget is
five attempts. `just manage retry-job <uuid>` explicitly grants another five
attempts while preserving history, up to the schema's 1000-attempt bound. A failed
job remains retryable only while its retained row exists.

Shutdown stops claiming, drains current work for a bounded period, then cancels
remaining futures. Interrupted claims recover after expiry. A heartbeat cannot
extend a job past its execution deadline. Handlers must respect cancellation;
blocking operations need their own bound. This is at-least-once execution, not an
exactly-once guarantee for external side effects.

## Authentication mail

The identity domain prepares an encrypted, short-lived delivery in the business
transaction. The mail crate renders MJML HTML and plain text through strict
Minijinja templates and sends through lettre SMTP. Pending delivery secrets use
key IDs and delivery-specific encryption context. Expired or invalidated actions
are not prepared for sending. Acceptance clears stored ciphertext.

SMTP runs outside database transactions. Acceptance means the SMTP server accepted
the message, not that it reached an inbox. A crash after acceptance but before
recording it can resend. Reuse the delivery UUID in Message-ID; do not assume this
forces downstream deduplication. Action-token consumption remains single use.
Account/challenge authority is checked again when the recipient uses the action.

## Retention and maintenance

`manage cleanup` removes expired authentication rows, finished jobs and their
attempt history, expired audit events, and due file lifecycle records in bounded
batches. Retention defaults are `SUCCEEDED_JOB_RETENTION_DAYS=30`,
`FAILED_JOB_RETENTION_DAYS=90`, and `AUDIT_RETENTION_DAYS=180`. Each accepts 1 through
36500 days. Job age is measured from `finished_at`; audit age uses `created_at`.

Each invocation deletes at most 100 finished jobs, with at most 1000 attempt rows
per job removed through the foreign key cascade, and at most 1000 audit events.
Pending and running jobs are never retention candidates. Final job deletion repeats
the terminal-state and age predicates after lock contention, preserving a job that
was retried or failed again recently while cleanup waited. Deleting a retained
failed job also removes its inspection history and ability to retry it; export or
investigate it before the configured deadline.

Use `just cleanup <environment>` from `deploy/` so audit cleanup receives the
restricted maintenance role. API, worker, and ordinary management commands keep
append-only audit privileges. Schedule cleanup explicitly; application startup
does not create a scheduler. The [deployment templates](../../deploy/README.md)
include a five-minute systemd timer for cleanup and search repair. A busy backlog
may require repeated runs. Review retention against the consuming application's
requirements before deployment.
