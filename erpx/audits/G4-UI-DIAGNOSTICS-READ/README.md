# G4 authenticated platform diagnostics read surface

Date: 2026-07-28

This packet adds the missing local platform diagnostics read adapter. It does
not grant customer-support capability access and does not expose the raw
diagnostics JSON payload to the console.

## Authoritative behavior

- `GetSupportDiagnostics` remains server-authorized and audited. Platform mode
  requires platform scope and the existing diagnostics permission; the
  server's snapshot builder remains the allow-list/redaction authority.
- The additive `SupportDiagnosticField` transport contains only bounded safe
  summary fields. The raw `payload_json` remains available only to the
  server-owned boundary, and the response also carries the authoritative
  schema and capture timestamp.
- The console sends authenticated actor/session metadata and the platform
  diagnostics query context. It rejects oversized pages, malformed metadata,
  unknown or secret-bearing display keys, and invalid values before rendering.
- The Dioxus route is explicitly read-only and distinguishes loading, denied,
  offline, error, and safe read states. No diagnostic command, cache authority,
  credential, capability secret, or customer payload is introduced.

## Verification

```text
cargo fmt --all                                PASS
cargo check -p erpx-server --offline           PASS
cargo check -p erpx-console --offline --features web PASS
cargo test -p erpx-server --offline            84 passed, 1 ignored
cargo test -p erpx-ui --offline                81 passed
cargo test -p erpx-console --offline --features web 75 passed
```

The current-tree serialized workspace suite also passed all non-ignored
workspace and doc tests. A fresh eight-database ignored PostgreSQL run passed
every local database target; its sole nonzero target was the intentional
credentialless G14 external-readiness preflight. The disposable databases were
removed automatically after the run.

The server's existing diagnostics transport/redaction tests and the new
console typed-wire test pass. Full G4 support intersection, customer-support
capability journeys, notification delivery, device/session execution,
credentialed vendor acceptance, MinIO/PITR, migration, and G0–G14 closure
remain open in the current G14 matrix.

No production directory activation, command-owner change, Folio write
retirement, or cutover occurred.
