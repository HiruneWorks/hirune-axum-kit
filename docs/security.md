# Security controls

This template uses OWASP guidance as a review baseline. It does not claim ASVS
certification or completeness for every application's threat model. Review the
actual source and tests when changing authentication. UUIDv7 is an entity identity,
not a secret or an access-control mechanism.

## Required review checks

| Area | Contract and evidence |
| --- | --- |
| Passwords | At least 15 Unicode characters, at most 1024 UTF-8 bytes, no truncation or composition rule. Argon2id uses 19 MiB, 2 iterations, and 1 lane with random salt. Blocking work retains its concurrency permit until it actually finishes. |
| Enumeration | Signup, verification resend, and reset requests return the same accepted response for eligible and ineligible accounts. Invalid logins share an authentication error; missing accounts use dummy password verification. |
| Abuse limits | PostgreSQL stores keyed hashes of client and identity rate keys. Client requests are limited before hashing/provider requests; identity limits survive failed authentication. Defaults are 60 requests per client and 10 attempts per operation/identity per minute, with explicit settings. There is no attacker-exhaustible global login quota. |
| Sessions | Random 256-bit opaque credentials; only hashes stored. Shared database authority, idle and absolute expiry, rotation, per-session and all-session revocation. Credential changes increment account security version and revoke existing sessions/challenges. |
| MFA | TOTP is optional for ordinary accounts and required for global administrators. Secrets are encrypted; accepted TOTP steps and hashed recovery codes are single use. Password and OIDC login share the gate, which is also checked at session issuance, session authentication, and final service authorization. New-device email challenges are excluded. |
| Sensitive actions | Require a valid session and recent full authentication (within 5 minutes), with final checks under account lock. Password changes require the current password. MFA removal and recovery-code regeneration additionally require an unused TOTP step or recovery code; administrators cannot remove MFA. |
| OIDC | Maintained protocol library; authorization code flow, PKCE S256, state, browser binding, nonce, issuer, audience, signature, expiry, and access-token hash validation. Explicit linking only; matching email does not merge accounts. |
| Mail secrets | Challenge hashes in authority tables; encrypted short-lived pending mail payloads with key IDs and row-specific context. Clear ciphertext after acceptance or expiry. Links use fragments; never log them. |
| Authorization | Cedar schema and policies validated at startup. Evaluation diagnostics fail closed. Membership-scoped list queries and explicit actor/resource checks. |
| Browser transport | Secure host-only production cookies, exact Origin validation, explicit CORS, bounded body size, no-store, and no secret response logging. |
| Failures | Storage and identity-provider outages are unavailable errors, never a successful anonymous fallback. Failed OIDC code exchanges consume state; start a new flow. |
| Input validation | JSON/query DTOs validate after typed deserialization and reuse domain value rules. Validation error parameters may contain secrets; return generic input errors without logging or serializing those parameters. Services remain responsible for mutable authority and repeat value checks for non-HTTP callers. |
| Retention | Cleanup defaults to 30 days for succeeded jobs, 90 days for failed jobs, and 180 days for audit events. Pending/running jobs are preserved. Audit deletion uses a separate maintenance role; runtime audit access remains append-only. |

See [identity behavior](behavior/identity.md), [transactions](transaction-boundaries.md),
and the [regression inventory](regressions.md) for executable coverage. A test name
documents coverage intent; milestone notes record whether it was actually run.

## Proxy and cache trust

Use the actual socket peer for client limits by default. Forwarding headers are
ignored unless the peer appears in `TRUSTED_PROXY_IPS`. A trusted proxy must overwrite
`X-Forwarded-For` with exactly one client IP. Multiple values, lists, missing values,
and invalid addresses from a trusted proxy are rejected. Restrict direct backend
network access when using a trusted proxy. NAT users share a client budget; tune
the application's policy deliberately for its audience. Network/WAF limits are
still needed for volumetric attacks before traffic reaches the database.

PostgreSQL is authoritative for authentication state. Redis is not required. The
OIDC public-key cache has a bounded lifetime, serialized refresh, unknown-key
refresh, and failed-refresh cooldown. It never caches session authority. Do not
add cached allow decisions without an explicit revocation contract.

`AUTH_IDENTITY_LIMIT_PER_MINUTE` defaults to 10 for each operation/identity;
`AUTH_CLIENT_LIMIT_PER_MINUTE` defaults to 60 shared across authentication routes
for each client. Both accept 1 through 100000. Counters are shared across processes
and use a common saturation ceiling above every supported budget, so a lower-limit
instance cannot reset or truncate a higher-limit instance's counter. Keep instance
settings consistent; changing a budget does not reset the active counter window.

## Administrator and recovery policy

Only the operator command `manage set-administrator <uuid> <true|false>` changes
the global administrator flag. Promotion requires an active, verified account
with confirmed TOTP enrollment. A changed flag advances the security version,
revokes sessions/challenges, and appends an audit event in one transaction. Access
to this command must be controlled by the deployment; there is no public role
administration endpoint. Organization ownership does not imply global administrator
status.

An existing administrator without TOTP cannot authenticate or use an old session.
The operator must demote that account before it can sign in, enroll TOTP, and be
promoted again. Password reset preserves MFA. Recovery-code regeneration requires
recent authentication and a fresh proof, replaces all ten codes atomically, and
revokes every session/challenge. Plaintext codes are returned once and never stored.
Support-assisted factor reset remains a separate product policy.

## Key rotation and production responsibilities

Add a new 32-byte encryption key under a new key ID and set it active. Retain old
keys while rows encrypted with them remain, including enrolled TOTP secrets.
Removing an old key early makes those rows unreadable; automatic re-encryption is
not provided. Rotate database, SMTP, and provider credentials using their own
deployment procedures. Use separate migration, runtime, and maintenance database
roles. Runtime must not update or delete audit events; maintenance adds the audit
deletion needed for retention without schema-owner privileges. Development Compose
credentials are public local fixtures.

External password-screening services are excluded by owner decision. A local
common/breached-password blocklist is deferred and is not implemented; current
password validation enforces the length contract above. Do not add a network
dependency or describe this as breach screening.

Before a public deployment, review recovery/support policy, edge abuse controls,
the configured retention periods, backups, TLS termination, and provider-specific
acceptance tests. The baseline does not include passkeys, account email changes,
administrator UI, or an email delivery guarantee. Do not describe these as
implemented merely because they appear in OWASP guidance.

Successful password changes and resets register an encrypted, tokenless security
notification in the same transaction as the credential change. It contains no
password. Existing sessions remain revoked and a normal login is required. The
notification remains eligible after account deactivation so deactivation cannot
suppress a pending password-change alert. Signup welcome messages require a
verified, active account; action mail still requires its live challenge/version.

## Sources

- [OWASP Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)
- [OWASP Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
- [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
- [OWASP Forgot Password Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html)
- [OWASP OAuth2 Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/OAuth2_Cheat_Sheet.html)
- [OpenID Connect library verification guidance](https://docs.rs/openidconnect/latest/openidconnect/)
