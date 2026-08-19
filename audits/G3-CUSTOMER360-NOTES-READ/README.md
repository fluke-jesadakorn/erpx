# G3 Customer 360 notes transport and bounded read surface

Date: 2026-07-28

This packet carries the existing authoritative Customer 360 notes read model through the
versioned platform transport and into the authenticated Dioxus console. It does not create a
second notes authority or add a mutation path.

## Authoritative behavior

- `SqlxControlPlane::get_customer_notes_page` reads `erpx_control.customer_notes` from the read
  database with deterministic `(created_at DESC, id DESC)` pagination and a one-row continuation
  probe.
- The application/server boundary validates the authenticated platform query context, customer
  scope, purpose, bounded page size, and complete RFC3339/UUID cursor before the SQLx read.
- Note bodies are redacted by the domain read model before transport. The response exposes only
  note UUID, author reference, creation timestamp, body, and the opaque next cursor.
- The response is append-only/read-only: no note command, local cache authority, credential, or
  signed object access is introduced.

## Transport and UI evidence

- `CustomerNote` and continuation fields are additive protobuf fields on
  `GetCustomer360DetailResponse`.
- The console rejects pages over 100, incomplete cursors, non-UUID continuation IDs, control
  characters, oversized values, and secret-bearing/invalid note rows.
- The shared UI model keeps only bounded safe note fields and preserves the authoritative cursor;
  it never performs local pagination or fabricates a second page.
- The Customer 360 detail surface renders the bounded page and requests the next page through the
  server cursor. Cursor state is bound to the customer UUID, so changing customers implicitly
  returns to the first page; no local notes list becomes authoritative.

## Focused verification

```text
cargo fmt --all --check                              PASS
cargo test -p erpx-ui --offline                      81 passed
cargo test -p erpx-console --offline --features web  74 passed
cargo check -p erpx-proto --offline                  PASS
cargo check -p erpx-ui --offline                     PASS
cargo check -p erpx-console --features web           PASS
cargo check -p erpx-server --offline                 PASS
```

This is a G3 continuation packet. Live external holding routing, multi-holding acceptance,
backfill/reconciliation, binding-revocation replay, and native/mobile deep-link acceptance remain
tracked by the current G14 matrix.
