# External API access

An API client belongs to an organization and represents a server application.
Its keys share explicitly allocated scopes and request budgets. The client does
not impersonate the person who created it: signing out, changing that person's
password, or deactivating their account does not revoke the organization's keys.
Revoke a key or its client to withdraw that access.

## Credentials and scope

The baseline key format is:

    hak_v1_<32 hexadecimal UUIDv7 characters>.<43 Base64url characters>

The prefix identifies this credential type and its format version. UUIDv7 is a
public lookup identifier. The secret comes from 32 cryptographically random
bytes, providing 256 bits of entropy. This string format is a project convention,
not an industry-standard API key format.

Send the whole key in an Authorization header using the Bearer scheme, over
HTTPS in production. The external extractor accepts exactly one header and
rejects a simultaneous session cookie. Query parameters never authenticate a
request. API keys cannot access account, organization-management, or administrator
endpoints; session credentials cannot authenticate external endpoints.

Only the issuance response contains the whole key. It uses the API's no-store
policy. PostgreSQL stores a domain-separated BLAKE3 hash of the complete
credential. Comparisons use constant-time equality. Neither key lists, audit
records, application tracing, nor OpenAPI examples contain real credentials.
If the creation response is lost, revoke that key and issue a replacement.

Each key has a required lifetime of 1 through 365 days. Revocation takes effect
at the next admission check; already admitted requests may finish. Client
revocation disables every associated key and cannot be undone by the public API.
Rotation means issuing a replacement, updating the caller, then revoking the
old key. It never resets usage.

The supported scope is organizations:read. An empty scope list denies all current
external operations. Unknown or duplicate scopes are rejected. The example GET
/api/v1/external/organization returns only the client's owning organization's
UUID, name, and version. It accepts no caller-selected organization UUID.
HEAD explicitly returns 405 without admitting or charging a request.
Expose another domain through its own concrete external operation and scope;
do not enable key authentication across the existing session router.

## Management and allocation

Organization owners use their normal authenticated session:

| Route under /api/v1/organizations/{organization_id} | Operation |
| --- | --- |
| POST /api-clients | Create a named client with explicit scopes |
| GET /api-clients | List client metadata with scoped cursor pagination |
| GET /api-clients/{client_id} | Read client metadata and current budgets |
| PUT /api-clients/{client_id} | Replace its name and scopes |
| DELETE /api-clients/{client_id} | Revoke the client |
| POST /api-clients/{client_id}/keys | Issue a named expiring key |
| GET /api-clients/{client_id}/keys | List key metadata with scoped pagination |
| DELETE /api-clients/{client_id}/keys/{key_id} | Revoke a key |

Cedar's organization-owner policy and final account/session checks apply.
Writes additionally require full authentication within five minutes.
Membership alone and global administrator status do not confer organization
ownership. Scoped identifiers cannot be used to manage another organization's
client or another client's key. Lifecycle and quota changes append resource-linked
security audit events in the same transaction.

New clients have zero request allocation. Owner input cannot set quotas.
A trusted operator assigns both limits through the management executable:

    just manage set-api-client-quota <client-uuid> <requests-per-minute> <requests-per-month>

Each limit accepts 0 through 1,000,000,000,000. Either zero disables external
admission with 403. Protect operator-command execution in the deployment.
Allocation is per API client, not an organization-wide aggregate or a key.
The public API never grants a paid entitlement or resets a counter.

To bound metadata growth, an organization can register at most 100 clients and
a client at most 100 keys. These counts include revoked and expired records.
The baseline retains those records and provides no archival operation; a
consuming project with long-lived rotation needs an explicit archival policy.

## Shared request budgets

The rate window is a fixed UTC minute. The longer allowance is a UTC calendar
month, resetting at 00:00 UTC on the first day of the next month. It is not a
rolling thirty-day period. All replicas and every key of a client share the
same PostgreSQL counters. Restarting an instance, issuing a key, changing scopes,
or changing limits does not erase usage. Lowering a limit below current usage
blocks subsequent admission until allocation increases or the period changes.

After checking the presented secret without authority locks, admission starts a
short transaction, locks the client and then the key, and repeats credential,
expiry, client-state, scope, and allocation checks. It reads database time after
those locks. Both counters and one UUIDv7 admission record commit atomically.
The organization service supplies the scoped read projection in that transaction.
No network request, response streaming, or external processing runs under these
locks.

An authenticated, authorized admitted HTTP attempt consumes one unit from both
budgets. Invalid credentials, missing scopes, disabled allocation, rejected
methods, and exhausted budgets consume none. A later response failure or caller
disconnect does not refund a committed admission. Repeating a GET consumes another
unit; request IDs correlate events and do not deduplicate charges. An arbitrary
Idempotency-Key header does not create idempotency. Future writes need a concrete
business-idempotency and charging contract.

An accepted response includes:

| Header | Meaning |
| --- | --- |
| X-RateLimit-Limit | Current minute limit |
| X-RateLimit-Remaining | Minute units left after admission |
| X-RateLimit-Reset | Next UTC minute, Unix seconds |
| X-Quota-Limit | Current calendar-month limit |
| X-Quota-Remaining | Monthly units left after admission |
| X-Quota-Reset | Next UTC calendar month, Unix seconds |

Exhaustion returns the shared 429 error envelope. The code is api_rate_limited
for the minute or api_quota_exceeded for the month, with Retry-After seconds
until the relevant boundary. Monthly exhaustion takes precedence if both limits
are exhausted. Owners can still read metadata and usage with their session.
Storage failure returns 503 and cannot admit an unaccounted request.

## Persistence, maintenance, and extension

api_clients owns configuration and current counters; api_keys owns hashed
credentials; api_usage records each committed admission with client, key,
operation, time, and request correlation. Runtime credentials may insert and
read usage records but must not update or delete them. Maintenance can delete
expired usage, with no UPDATE privilege. It removes at most 1,000 records per
cleanup run, using API_USAGE_RETENTION_DAYS (default 90, range 1 through 36,500).
Retention does not change counters. Admission records are operational history,
not an invoicing ledger or an exactly-once billing system.

Human management locks account, organization, client, then key. External reads
lock client then key and perform a plain scoped organization read; they do not
subsequently take an organization lock. The current organization domain has no
suspension or deletion operation. New organization policies, deletion, or external
writes must extend the authority and lock-order contract before implementation.

Per-client locking deliberately serializes strict allocation decisions. The
concurrency tests cover multiple services and real API processes sharing one
database; they do not establish a production request-per-second capacity. Measure
the intended client concurrency and database budget before changing the backend.

An edge gateway or WAF can reject volumetric and unauthenticated abuse before
it reaches the application. APISIX, Cloud Armor, or Apigee are deployment choices,
not required dependencies. Edge limits do not replace the application's strict
allocation authority. Redis, an OAuth authorization server, subscription billing,
and a developer portal are not included in this baseline.

## References

- [RFC 6750 Bearer transport](https://www.rfc-editor.org/rfc/rfc6750.html)
- [GitHub credential prefixes](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/about-authentication-to-github)
- [OWASP REST Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/REST_Security_Cheat_Sheet.html)
- [OWASP API resource consumption](https://owasp.org/API-Security/editions/2023/en/0xa4-unrestricted-resource-consumption/)
- [Cloud Armor approximate edge limits](https://docs.cloud.google.com/armor/docs/rate-limiting-overview)
- [Apigee quota policy](https://docs.cloud.google.com/apigee/docs/api-platform/reference/policies/quota-policy)
- [APISIX consumer groups](https://apisix.apache.org/docs/apisix/terminology/consumer-group/)
