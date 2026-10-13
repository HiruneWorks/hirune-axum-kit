# Authorization

Cedar schema and policies are loaded and validated at bootstrap. Invalid policy
prevents startup. Evaluation diagnostics are internal failures and deny the
operation; they do not become a permissive fallback.

An authenticated actor must be active and verified. Self-account operations use
the account policy after a fresh database check. Global administrator status does
not imply membership in every organization. The organization example permits
members to read and its owner to manage; non-member detail access is hidden as 404.
The owner membership is created atomically with the organization.

Organization lists apply membership scope inside the query before pagination.
Do not fetch an unrestricted page and filter it afterward. Mutation services lock
the actor before the organization, recheck access, and require the expected
resource version. Stale updates return conflict rather than overwriting changes.

The example deliberately has no general permissions editor, role hierarchy,
membership invitation flow, or administrator API. Add product operations through
their domain service, policy, scoped queries, and tests together. Never infer
authorization from UUID possession or from a cached session principal alone.
