# G3 Customer 360 detail redaction boundary

## Delivered behavior

The authenticated Customer 360 detail API now returns typed section-state
metadata (`Customer360SectionState`) and leaves the legacy opaque
`payload_json` field empty. The adapter no longer retains or renders that blob;
it is an aggregate of multiple server-owned sections and is not a client
display contract. The console now:

- validates customer identity, freshness, projection metadata, bounded notes,
  and unsupported reasons;
- accepts only the closed Customer 360 section names and their `Present`,
  `Absent`, or `Unsupported` state;
- retains no raw projection bytes, JSON strings, arbitrary keys, credentials,
  account details, or provider material; and
- renders safe section-state metadata plus the existing bounded, cursor-bound
  notes read model and freshness/read-only state.

Malformed, oversized, incomplete, or unknown section payloads fail closed.
The server derives section states from the authoritative typed
`Customer360DetailReadModel`; no detail aggregate JSON is serialized into the
response. It remains authoritative for customer scope containment, global-read
authorization, SQLx control read persistence, audit recording, projection
freshness, notes pagination, and rebuild/reconciliation behavior.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-console --offline --no-fail-fast --quiet` — PASS,
  103 tests.
- `cargo clippy -p erpx-console --all-targets --offline -- -D warnings` —
  PASS.
- `cargo test -p erpx-server --bin erpx-server --offline --no-fail-fast` —
  PASS, 85 passed, 1 ignored.

The focused redaction test proves secrets and arbitrary projection fields do
not appear in the client display model or its debug representation. Live
multi-holding/customer binding, device execution, and broader G3/G14
acceptance remain governed by their existing certification gates.

## Rollback

Rollback is limited to restoring the previous console adapter source. No
database, protobuf, server authorization, worker, projection, or Folio write
state is changed by this packet. The authoritative read source and audit
history remain intact.
