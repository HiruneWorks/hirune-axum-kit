# Transaction boundaries

An application service starts and commits its transaction. Repository writes
accept that transaction, not a database pool. A composed operation uses the
owning service's explicit `*_in_transaction` operation, as demonstrated by
organization creation. Returning an error drops and rolls back the transaction.

## Lock order

Lock the acting account first, then the domain resource. Re-read the current
session, token hash, account flags, security version, and required recent
authentication under that account lock before sensitive operations. Organization
mutations then lock the organization and check membership, policy, and version.
Never reverse this order in another endpoint or worker.

Email and MFA challenges are discovered before locking their account, then
re-read and consumed conditionally under the account lock. Expiry is checked at
consumption. TOTP replay state belongs to the account, not an individual challenge;
recovery code consumption is conditional and single use. OIDC callback state is
bound to a browser and consumed before exchanging the code with the provider.
An ambiguous exchange failure requires a new authorization flow.

Password verification and hashing run on bounded blocking tasks before database
locks. After locking, compare the password snapshot and security version again.
Network requests, provider discovery, SMTP, and template rendering do not run
while holding an account or resource lock.

## Durable effects

Signup and recovery commit account/challenge state, an encrypted delivery, and a
durable job together. The job contains a delivery UUID, not a token, recipient,
or mail body. The worker renders and sends after the transaction commits.

SMTP acceptance and a database commit cannot be atomic. A crash between them can
send the same mail again. Preserve the delivery's Message-ID and describe this as
at-least-once delivery; a stable identifier does not prove recipient deduplication.
The action itself remains single use and authority is checked when it is consumed.

Rate-limit counters commit in their own short transaction. A later invalid
password, challenge, or rolled-back business change must not refund the attempt.
Job claims and completion have their own short transactions and use claim-token
fencing, database time, leases, and hard execution deadlines.

Use `tests/identity.rs` for atomic composition and credential races,
`tests/jobs.rs` for lease and retry behavior, and `tests/process.rs` for the
separate-process SMTP boundary. Do not replace these with database mocks.
