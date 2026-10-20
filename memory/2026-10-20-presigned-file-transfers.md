# 2026-10-20 — Presigned original-file transfers

## Changes

- Replaced API byte uploads and streaming downloads with authenticated upload URL,
  completion, and GET/HEAD URL operations. Removed the old `/content` routes and
  temporary-file transport. This is an intentional client contract change.
- Kept organization quotas, uploader checks, current session/membership authority,
  same-organization Ready attachments, filename search, and durable deletion.
- Added separate internal and client-facing S3 endpoints. Explicit integration
  provisioning configures bucket CORS from ALLOWED_ORIGINS.
- Updated environment examples, behavior/requirements, OpenAPI, attachment tests,
  and the two-API k6 workflow. No schema migration or content processing was added.

## Decisions and constraints

- Default upload URLs last 300 seconds; download URLs last 60 seconds. Reissuing
  an upload URL does not extend the original reservation deadline. An issued URL
  remains a bearer capability until expiry. Loaded content does not disappear.
- PUT signatures bind the exact length, conditional creation, octet-stream type,
  identity encoding, attachment disposition, and no-store. Completion verifies
  HEAD metadata outside locks, then repeats authority/state checks under locks.
  Concurrent completion is idempotent. New files have no verified BLAKE3 hash.
- Keep Deleting files quota-reserved through the reservation deadline plus the
  configured writer grace. Storage ingress must bound total PUT duration within
  that grace. Deletion must start after the horizon before it can refund quota.
- Astra reviewed the design and implementation. The review identified the
  cleanup timing race and the Docker-facing signing endpoint; both were fixed.
- Native clients must send Content-Length explicitly for an empty PUT. Hyper can
  omit the header for an empty body. Browsers derive it from File/Blob instead.
- Direct retrieval remains an attachment/original policy. A consuming application
  owns safe inline media behavior and URL refresh after expiry.

## Validation

- `just check` passed formatting, boundaries, documentation links, Clippy with
  warnings denied, workspace unit/doc tests, and OpenAPI drift. The final run used
  RUST_TEST_THREADS=1. An earlier parallel run failed one existing local
  siteverify-retry test; the serial rerun passed without authentication changes.
- `just test-files` passed nine real PostgreSQL/SeaweedFS/Meilisearch tests. They
  cover signed metadata, rejected length/type/ACL changes, concurrent conditional
  PUT, zero bytes, idempotent completion, current authority, GET/HEAD separation,
  ranges, CORS preflight, expiry/reissue, quotas, late writes, and cleanup locking.
  The writer-horizon regression synchronizes on the blocked DB lock instead of
  relying on a short wall-clock delay.
- `just test-content` passed all 14 tests, including direct-upload attachments,
  organization isolation, deletion, and attachment-link cleanup.
- `just load-test` passed the local two-user smoke profile: 146 requests, 228
  successful checks, no failed requests/checks, and p95 490.69 ms. The isolated
  fixture advances writer deadlines after all PUTs finish, then runs canonical
  reconciliation. This debug-build result does not establish production capacity.
- `cargo check --locked --tests` and `git diff --check` passed.

## Remaining work

- Actual browser File/Blob integration and public storage ingress acceptance were
  not run. Configure client reachability, bucket privacy, CORS, nosniff, query-log
  redaction, and a finite ingress PUT deadline before deployment.
- Remote CI, pushing, publication, and deployment are separate and were not run.
