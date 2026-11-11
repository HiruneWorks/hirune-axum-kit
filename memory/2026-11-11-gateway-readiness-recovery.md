# 2026-11-11 — Gateway readiness recovery

## Changes

- Issue [#47](https://github.com/HiruneWorks/hirune-axum-kit/issues/47) tracks the
  shared gateway recovery defect exposed by downstream V7's Kubernetes CI.
- Readiness probes now inspect HTTP status without parsing transient HTML gateway
  error bodies as JSON. Gateway health and maintenance-error probes still validate
  their expected JSON fields.
- Backend verification retries within a 180-second budget, with two seconds
  between failures. Every gateway must return HTTP 204 in one stable Pod set
  before maintenance can be disabled. Timeout errors retain safe gateway/status
  context without the response body.
- A Unix regression starts an isolated child test process with a kubectl fixture.
  One of two gateways first returns HTML HTTP 502, then recovers. The test requires
  both gateways to pass before maintenance is disabled and checks response-body
  secrecy. The subprocess keeps fixture PATH changes out of concurrent tests.
- Updated the deployment runbook. No schema, public HTTP contract, environment
  setting, image version, or request/drain budget changed.

## Decisions and constraints

- Preserve existing maintenance-state interpretation and rollout rules.
- Keep maintenance enabled during unsuccessful recovery. Do not accept only the
  first ready gateway or a changed gateway Pod set.
- The retry budget does not impose an outer timeout on Kubernetes processes.
  In-flight Kubernetes calls can extend total command time. This is a documented
  limitation, not a strict wall-clock deadline guarantee.
- Kit PR #45 passed its full CI and merged before this transient-response defect
  was found. This follow-up is a separate correction.

## Validation

- Before correcting the implementation, the regression reproduced
  `expected value at line 1 column 1` on Kit main's gateway helper.
- `cargo test --locked --package xtask`: all four tests passed after the fix.
- Strict all-target xtask Clippy, workspace rustfmt checking, shell syntax, and
  `git diff --check` passed on Rust 1.98.1.
- `cargo build --locked --package xtask` and `xtask test-deployment` passed locally
  with kubectl 1.37.1 and Docker Compose 2.40.3.
- Focused local execution was limited to deployment tooling. Broader local
  application/provider/Kubernetes suites were not run for this correction.
- Astra reviewed the equivalent V7 source correction and found no blocker. The
  review was read-only. It noted the outer-timeout limitation and optional
  permanent-failure and gateway-membership regression coverage.
- Hosted CI, merge, publication, and live deployment remain separate.

## Remaining work

- Complete the follow-up PR's hosted CI. Do not infer live-cluster acceptance from
  deterministic kubectl fixture responses.
- Keep V7 adoption in sevenwiki/V7#673 separate from this template correction.
