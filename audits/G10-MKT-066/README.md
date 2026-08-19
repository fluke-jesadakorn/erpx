# G10-MKT-066 — Production Marketing callback secret-manager adapter

## Delivered behavior

The server can now compose `HttpMarketingCallbackSecretManager` for
`ERPX_MARKETING_CALLBACK_SECRET_RESOLVER=http` or `secret-manager`.

The adapter requires a bounded HTTPS endpoint with no userinfo, query, or
fragment, disables redirects, applies a bounded timeout, and optionally sends
an injected bearer credential without exposing it in `Debug` output. A
validated `secret://`, `sm://`, `vault://`, or `aws-sm://` reference is appended
as one encoded path segment under the fixed `v1/secrets` route. `env:NAME`
references fail closed in this production adapter.

Responses are bounded before JSON parsing and accept only one of the two
allow-listed shapes: `{ "value": "..." }` or `{ "secret": "..." }`.
Non-success responses, network failures, malformed/unknown response shapes,
and invalid material map to redacted resolver errors. Resolved bytes are
validated by `ResolvedMarketingCallbackSecret` and remain zeroizing. No
database or holding route is accessed by this adapter.

## Evidence

- `cargo fmt --all --check` — PASS.
- Infrastructure adapter tests — 4 PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- Server callback transport tests — 3 PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-server` — PASS.
- `git diff --check` — PASS.

The focused tests cover HTTPS endpoint validation and redaction, encoded
reference binding, response-shape/size policy, and fail-closed env references.

## Scope and remaining work

This packet provides a vendor-neutral HTTPS gateway contract. Vendor-native
AWS Secrets Manager/Vault adapters, endpoint health/rotation reconciliation,
deployment identity binding, and a live TLS integration proof remain open.
The broader Marketing/ERP, migration, and Folio write-retirement gates remain
open.
