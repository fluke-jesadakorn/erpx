# G4-007 diagnostics redaction evidence

## Delivered behavior

`erpx-infrastructure::support_diagnostics` provides an allow-list diagnostics
boundary for versions, projection lag, failed-job summaries, integration
errors, worker/backup health, correlation IDs, and safe configuration labels.
Inputs are private staging values; output is bounded, deterministic, deduplicated,
and serializable only after forbidden material has been classified and removed.

Credentials, secrets, private keys, payment instruments, tokens, and unrelated
business data are excluded. Output limits, per-section caps, safe identifiers,
and deterministic truncation are enforced.

## Independent evidence

```text
cargo fmt --all --check                                      PASS
cargo test -p erpx-infrastructure support_diagnostics::tests  PASS — 6 tests
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings PASS
cargo test -p erpx-server                                       PASS — 42 tests
```

## Remaining boundary

The authenticated `SupportService.GetSupportDiagnostics` gRPC route now
requires platform scope, the `diagnostics:read` role permission, a validated
purpose, and query attribution; it records a platform audit event and returns
only the bounded serialized snapshot. Support-ticket capability-bound
diagnostics, live worker/backup producers, incident workflow, and HTTP/native
support surfaces remain required for G4 completion.

## Rollback

Remove `support_diagnostics.rs`, its `infrastructure/src/lib.rs` re-export, and
this evidence directory. No database or runtime state is changed.
