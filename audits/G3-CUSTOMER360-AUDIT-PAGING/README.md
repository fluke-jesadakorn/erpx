# G3 Customer 360 access-audit cursor pagination

Date: 2026-07-28

This packet completes the local console adapter for the existing authoritative
customer access-audit page. The server already owns the global-read audit,
scope authorization, SQLx projection, ordering, and cursor production; this
slice makes the existing next cursor usable in the authenticated UI.

## Authoritative behavior

- `GetCustomerAccessAudit` remains the only read boundary. It records the
  customer-visible global-read audit and validates the exact customer scope,
  page size, and paired RFC3339/UUID cursor before SQLx reads.
- The console validates the bounded response, rejects incomplete/non-UUID
  cursors and control/oversized display values, and retains only the existing
  safe actor/module/scope/purpose/time/correlation fields. Session references,
  customer identifiers, and raw payloads remain outside the view model.
- The next-page control sends the server-produced cursor and binds it to the
  customer UUID. Changing customer routes or selecting another customer
  implicitly returns to the first page; local lists never become authoritative.
- The surface remains read-only and exposes the server's stale, denied,
  loading, empty, and error states. No audit mutation, cache authority, or
  credential is added.

## Verification

```text
cargo fmt --all --check                                      PASS
cargo test -p erpx-console --offline --features web           76 passed
```

The existing server SQLx/global-read audit and projection tests remain in the
workspace suite. Broader G3 multi-holding, notes/audit backfill,
binding-revocation reconciliation, and native/mobile deep-link acceptance
remain open.

No production directory activation, command-owner change, Folio write
retirement, or cutover occurred.
