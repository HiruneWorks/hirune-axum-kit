# Identity

Accounts use normalized email as a unique login identifier. Entity IDs are UUIDv7.
Password signup creates an inactive-for-login, unverified account and queues a
verification email atomically. Verification consumes one expiring challenge.
Resending replaces the previous verification challenge and only sends for pending
accounts. Duplicate signup and ineligible resend/reset requests return accepted
without disclosing whether an account exists.

Password login verifies credentials before locking, then rechecks the account and
credential snapshot under lock. An account must be active and verified. If TOTP
is enabled, primary login returns a short-lived MFA challenge rather than a
session. Password and OIDC login use this same final gate. The baseline does not
send a separate confirmation email for every new device.

Sessions are opaque credentials backed by PostgreSQL, with idle and absolute
expiry. Activity extends idle expiry at most once per minute and never beyond the
absolute deadline. Rotation replaces the credential; the old token stops working
across instances. Lists and revocation are owner-scoped. Account deactivation,
password reset/change, and MFA configuration changes invalidate existing sessions
and outstanding challenges. Deactivation is not physical data erasure.
Authentication maintenance deletes expired rows in bounded batches. Session
deletion rechecks both deadlines after a concurrent activity update releases its
row lock, so an accepted idle extension cannot be removed from an old snapshot.

Password reset is available for verified active accounts with a password. It is
single use, changes the password, and invalidates sessions in one transaction.
It does not remove TOTP. OIDC-only accounts do not gain a password through reset.
Changing a password requires the current password and recent authentication.

## MFA and recovery

TOTP enrollment requires recent authentication, an expiring setup challenge, and
a valid authenticator code. Only after confirmation is MFA enabled. The caller
receives ten recovery codes once; only hashes are stored. TOTP secrets are
encrypted with the active application key and account-specific context.

Accepted TOTP steps are tracked on the account, so reusing a step through another
challenge fails. Recovery codes and MFA challenges are consumed atomically under
the account lock. Removing MFA requires recent authentication plus an unused
TOTP step or recovery code, and revokes sessions. Keep the returned recovery
codes outside the authenticated browser session. Recovery-code regeneration and
support-assisted recovery are not baseline operations.

## OIDC

Configured OIDC providers use authorization code flow and PKCE S256. State,
nonce, verifier, and browser binding are persisted in a five-minute flow; sensitive
flow material is encrypted. A callback can be handled by another API instance.
Callback state is consumed before external code exchange, including cancellation.
Retry an interrupted login by starting a new flow, not replaying its code.

The protocol library checks signed ID token issuer, audience, nonce, signature,
expiry, and access-token hash when present. Provider metadata/JWKS refresh after
five minutes and on an unknown signing key, with serialized refresh and a five-second
retry cooldown measured from completion or cancellation. Transport/provider
failures return unavailable; invalid grants
and claims return unauthenticated. Optional `iss` callback parameters are checked.

A returning identity is matched by exact issuer and subject. A new identity
requires a verified provider email and cannot silently claim an existing local
email address. Linking starts from a recent authenticated session, and callback
rechecks that initiating session and security version. Unlinking cannot remove an
account's last login method. Provider access tokens are not stored.

These controls are exercised in `tests/identity.rs`, `tests/api.rs`,
`tests/oidc.rs`, and `tests/process.rs`. Live provider registration, consent,
redirect configuration, and email delivery outside Mailpit remain deployment
acceptance work.
