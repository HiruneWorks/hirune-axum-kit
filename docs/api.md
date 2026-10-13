# API conventions

The production router exposes `/api/v1/`, `/health/live`, `/health/ready`, and
`/openapi.json`. [openapi.json](openapi.json) is generated from that router's typed
contracts with `just openapi`; `just check` rejects drift. Do not hand-edit it.

HTTP handlers extract typed input, authentication context, and request correlation,
call an owning application service, and translate its result. They contain no
database queries or business policy. Return DTOs rather than persistence models.
Normal input objects reject unknown fields; the OIDC callback accepts protocol
extensions. Malformed JSON, UUIDs, and query values use the same error envelope.

## Typed input and validation

`ApiJson<T>`, `ApiQuery<T>`, and `ApiPath<T>` deserialize concrete input types and
translate extraction failures into the shared API error. Serde enforces field
types and required fields; normal JSON DTOs use `deny_unknown_fields`. `ToSchema`
describes OpenAPI and does not perform runtime validation. The extractors do not
automatically call a DTO validation trait.

Owning services validate values such as normalized email, password length,
organization names, and file metadata. Authorization and rules that depend on
current state remain in those services, with final checks under their locks.
These checks also apply when a worker or management command calls the service
without HTTP. Any future DTO validation must preserve this service boundary.

## Errors and headers

Errors have this shape:

```json
{"error":{"code":"unauthenticated","message":"Authentication is required."},"request_id":"019..."}
```

The server generates a UUIDv7 request ID and returns `X-Request-Id`. Caller-supplied
request IDs are not trusted. Expected failures use 400, 401, 403, 404, 409, or 429;
storage/provider unavailability uses 503. Internal details and secrets are never
returned. Middleware normalizes router and extractor errors too. All API responses
use `Cache-Control: no-store`, `nosniff`, a restrictive CSP, and no-referrer.

JSON bodies are bounded at 16 KiB. Requests have a configured deadline and a
bounded concurrency budget; overload returns 503. Invalid attempts count toward
authentication limits. A 429 includes `Retry-After: 60`.

## Authentication transports

Browser login defaults to an HttpOnly SameSite=Lax cookie and requires an allowed
Origin. Production cookies also use Secure, the `__Host-` prefix, and Path=/.
Unsafe cookie-authenticated requests require an exact allowed Origin. CORS uses
explicit origins; it is not an authorization mechanism.

Native clients explicitly request `transport: "bearer"` at login and MFA completion
and receive the same opaque session credential in JSON. Send it as `Authorization:
Bearer <token>`. It is not a JWT. Cookie and bearer credentials together, duplicate
session cookies, and multiple Authorization headers are rejected. Session rotation
preserves the request's transport. Never put credentials in query strings.

## Pagination and updates

Lists use `limit` (default 25, range 1–100) and an optional opaque `cursor`. Results
are `{ "items": [...], "next_cursor": null }`, ordered by ascending UUID. Queries
fetch at most limit+1 rows and always apply the owner/membership scope in SQL.
No implicit count query is made. A cursor has a version and scope, but is not an
authorization credential; every query still enforces access. Unknown versions,
invalid encoding, and mismatched scopes are invalid input.

Organization updates require `expected_version`; a stale value returns 409.
Current operations have explicit full input fields. If a future partial-update
contract accepts both omission and null, represent and test those states separately.
Unauthorized organization detail access returns 404, including for administrators
who lack membership. List results must not leak the existence of other scopes.
