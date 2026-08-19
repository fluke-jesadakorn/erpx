# G10-MKT-065 — Marketing callback secret-resolution boundary

## Delivered behavior

Marketing provider-account callback bindings now cross the application boundary
with a validated opaque secret reference. The domain accepts only bounded
`secret://`, `sm://`, `vault://`, `aws-sm://`, or explicitly named `env:NAME`
locators. It rejects raw secret material, controls, whitespace, query and
fragment components, traversal, malformed environment names, and oversized
references without retaining the rejected value in errors or `Debug` output.

The HTTP callback handler resolves the reference through an injected
`MarketingCallbackSecretResolver`, verifies the exact raw request body, and
keeps resolved bytes inside a zeroizing container whose scope ends immediately
after HMAC verification. Resolver failures are mapped to a generic
configuration response. The provider-account binding remains authoritative in
the control plane, and holding routing occurs only after signature verification.

The host-native pilot composition supports `env:NAME` through
`EnvMarketingCallbackSecretResolver`. `secret://`, `sm://`, `vault://`, and
`aws-sm://` references fail closed until a production secret-manager adapter is
injected. `ERPX_MARKETING_CALLBACK_SECRET_RESOLVER=unconfigured` is available
for explicit fail-closed deployments; the default remains the env pilot for
local compatibility.

## Evidence

- `cargo fmt --all --check` — PASS.
- Domain callback-reference tests — 5 PASS.
- Infrastructure resolver tests — 3 PASS.
- Server callback raw-body/signature tests — 3 PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `git diff --check` — PASS.

## Scope and remaining work

This packet establishes the validation, injection, zeroization, and fail-closed
composition boundary. A production external secret-manager implementation,
its rotation/reconciliation policy, and a live end-to-end callback proof using
that adapter remain open. This packet does not claim the broader Marketing,
ERPX, migration, or Folio write-retirement gates complete.
