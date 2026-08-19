# G1-004 evidence

The server boundary tests are in `erpx/apps/server/src/boundary.rs`.

They cover mandatory context presence, actor/session attribution, contract version,
holding scope, query consistency, audit correlation, and pre-handler rejection.
The disposable boot check additionally verifies SQLx migrations and the HTTP
`/healthz`, `/readyz`, and `/metrics` endpoints on a separate server process.

The current follow-up also adds:

- centralized access-token actor/session attribution in `AuthContext`;
- bounded audit purpose/module validation;
- fail-closed customer organization scope binding before customer-scoped
  commercial commands;
- stable ERPX error-code/retryability/correlation metadata for application
  failures; and
- corrected role-renewal scope containment.

`cargo test -p erpx-server -- --nocapture` and
`cargo clippy -p erpx-server --all-targets -- -D warnings` pass. The HTTP
boundary now normalizes every non-success response to a serialized
ErrorContext body with stable code, safe message, correlation ID, retryability,
and details fields. Tonic failures from the transport interceptor and every
typed RPC handler carry the same serialized context in `x-erpx-error-context`
metadata plus stable code/retry/correlation metadata. Responses also carry
correlation/security headers, health endpoints enforce GET, a redacted request
trace records only method, route, correlation, and latency, and identity
readiness probes the write-side pool. Structured tracing across every
worker/holding path, provider/raw-body audit retention, readiness probes for
worker/commercial/holding state, and a complete redaction audit remain open.
