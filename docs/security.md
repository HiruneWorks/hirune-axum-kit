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
| Abuse limits | PostgreSQL stores keyed hashes of client and identity rate keys. Client requests are limited before hashing/provider requests; identity limits survive failed authentication. There is no attacker-exhaustible global login quota. |
| Sessions | Random 256-bit opaque credentials; only hashes stored. Shared database authority, idle and absolute expiry, rotation, per-session and all-session revocation. Credential changes increment account security version and revoke existing sessions/challenges. |
| MFA | Optional TOTP with encrypted secret, per-account step replay prevention, single-use hashed recovery codes, and expiring challenges. Every primary login method uses the same final MFA gate. New-device email challenges are excluded. |
| Sensitive actions | Require a valid session and recent full authentication (within 5 minutes), with final checks under account lock. Password changes require the current password; MFA removal additionally requires TOTP or a recovery code. |
| OIDC | Maintained protocol library; authorization code flow, PKCE S256, state, browser binding, nonce, issuer, audience, signature, expiry, and access-token hash validation. Explicit linking only; matching email does not merge accounts. |
| Mail secrets | Challenge hashes in authority tables; encrypted short-lived pending mail payloads with key IDs and row-specific context. Clear ciphertext after acceptance or expiry. Links use fragments; never log them. |
| Authorization | Cedar schema and policies validated at startup. Evaluation diagnostics fail closed. Membership-scoped list queries and explicit actor/resource checks. |
| Browser transport | Secure host-only production cookies, exact Origin validation, explicit CORS, bounded body size, no-store, and no secret response logging. |
| Failures | Storage and identity-provider outages are unavailable errors, never a successful anonymous fallback. Failed OIDC code exchanges consume state; start a new flow. |

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

## Key rotation and production responsibilities

Add a new 32-byte encryption key under a new key ID and set it active. Retain old
keys while rows encrypted with them remain, including enrolled TOTP secrets.
Removing an old key early makes those rows unreadable; automatic re-encryption is
not provided. Rotate database, SMTP, and provider credentials using their own
deployment procedures. Use separate migration and restricted runtime database
roles. Development Compose credentials are public local fixtures.

Before a public deployment, choose a breached/common-password screening policy,
MFA enforcement for privileged users, recovery/support policy, edge abuse controls,
retention and backups, TLS termination, and provider-specific acceptance tests.
The baseline does not include a breached-password database, passkeys, account
email changes, administrator UI, or an email delivery guarantee. Do not describe
these as implemented merely because they appear in OWASP guidance.

## Sources

- [OWASP Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)
- [OWASP Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
- [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
- [OWASP Forgot Password Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html)
- [OWASP OAuth2 Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/OAuth2_Cheat_Sheet.html)
- [OpenID Connect library verification guidance](https://docs.rs/openidconnect/latest/openidconnect/)
