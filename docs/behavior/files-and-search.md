# Files and search

Files are organization attachments. Original bytes live in a private SeaweedFS
S3 bucket; PostgreSQL owns UUIDv7 metadata, uploader identity, organization scope,
quota reservations, and state. A filename is display metadata, never an object
key or filesystem path. Uploads do not execute, unpack, sanitize, scan, or convert
content. `ready` means storage completed, not that the content is harmless.

## Upload and access

Reserve a file through the authenticated organization API, then PUT its raw bytes
once. Current membership is required for reservation, finalization, reads, and
deletion. Only the uploader may send the bytes; the uploader or organization owner
may delete an attachment while still a member. A file UUID does not grant access.

The service reserves declared bytes under the organization lock before accepting
the body. It checks actual streamed bytes, deadlines, and quota independently of
client headers. A bounded temporary file permits a streaming S3 PutObject without
buffering the file in memory or leaving incomplete multipart uploads. Transfer
slots bound temporary disk use; temporary files are removed on normal completion
and request cancellation. Operators must budget temporary disk and clean stale
temporary files after process crashes.
Byte and live-file quotas, a pending-reservation cap, and a per-uploader reservation
rate apply under the same locks, including for zero-byte files. Defaults are
100 MiB per file, 1 GiB/10,000 live files per organization, 100 pending reservations,
and 30 reservations per uploader per minute. These limits are configurable.

State transitions are `uploading -> transferring -> ready -> deleting -> deleted`.
An upload key is generated once and never reused. Storage I/O occurs outside DB
locks. Finalization rechecks session, membership, deadline, and state under the
same account -> organization -> file lock order. Follow the
[transaction boundaries](../transaction-boundaries.md): parent
business locks allow foreign-key existence checks from background file updates.
Expired/failed transfers remain inaccessible and are reconciled by maintenance.
Deletion immediately denies new
reads; durable jobs remove bytes. Retained tombstones permit repeated cleanup of
late uploads. Cleanup schedules repeat deletion at most once per hour per
tombstone. After the configured retention (30 days), metadata can be purged only
after recent successful storage deletion and satisfaction of any persisted
index-deletion obligation. Disabling search does not erase that obligation;
re-enable the configured index and repair it before purging affected metadata.
Existing streams authorize at their start; already delivered bytes
cannot be revoked.

Original downloads use `application/octet-stream`, attachment disposition,
`nosniff`, and no-store. GET/HEAD support a single byte range, including suffix and
open-ended ranges. Multiple ranges are rejected. This baseline provides original
retrieval; a consuming app can define a separate inline media policy. Never trust
the uploader's declared media type to enable HTML/SVG execution on the app origin.

## Derived filename search

Meilisearch indexes file IDs, organization IDs, and filenames, not file contents.
Keys stay server-side. Organization filters come from the authorized route scope.
Search returns candidate IDs; the service hydrates current ready rows from
PostgreSQL and checks membership again before returning metadata. Index totals
and snippets are not exposed. Membership removal and file deletion take effect
even when the index is stale.

Ready/deleting transitions enqueue indexing jobs in their DB transaction. Workers
wait for each asynchronous Meilisearch task to complete, then recheck current
state/revision and repeat reconciliation when necessary. An ambiguous network
failure is retryable. Maintenance can enqueue a bounded repair batch to correct
late writes or an index rebuilt after data loss. Searches are eventually
consistent; PostgreSQL is always authoritative for access.

## Integration boundary

The example scope is an organization. A messenger must authorize attachments
through current conversation/message access in its owning service. Original
upload ownership must not become a permanent bypass for parent permissions.

- [OWASP file upload guidance](https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html)
- [AWS Rust S3 examples](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/rust_s3_code_examples.html)
- [SeaweedFS S3 API](https://github.com/seaweedfs/seaweedfs/wiki/Amazon-S3-API)
- [Meilisearch asynchronous tasks](https://www.meilisearch.com/docs/capabilities/indexing/tasks_and_batches/async_operations)
