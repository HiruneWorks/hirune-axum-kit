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
attempts while preserving history, up to the schema's 1000-attempt bound.

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

`just manage cleanup` removes bounded batches of expired authentication rows and
successful jobs older than 30 days, including their attempt history. Failed jobs
are retained for inspection/retry. Arrange the cleanup command through deployment
scheduling; API/worker startup does not create a hidden scheduler. A busy backlog
may require repeated cleanup runs. Set product-specific audit and failed-job
retention before production operation.
