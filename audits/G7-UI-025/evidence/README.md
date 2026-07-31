# G7-UI-025 shared Operations presentation evidence

The shared Dioxus UI crate now provides a client-only Operations navigation and
display contract. It does not create source transactions or decide server
authorization.

## Delivered behavior

- Registers all required `/operations` pages in stable order with Thai/English
  labels and route-agnostic navigation items.
- Carries holding/company/branch/subtree scope, freshness, and explicit
  read-only metadata.
- Allow-lists display fields and redacts suspicious credential/URL material.
- Represents loading, empty, ready, denied, stale, read-only, conflict,
  offline, and error states without collapsing them.
- Enumerates only source domains that may feed Operations; Operations itself is
  never a source owner.

## Independent verification

Run from `erpx/`:

| Command | Result |
|---|---|
| `cargo fmt -p erpx-ui -- --check` | PASS |
| `cargo test -p erpx-ui` | PASS; 32 tests |
| `cargo clippy -p erpx-ui --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS after Main integration |

## Open work and rollback

The model is not wired to Operations transport DTOs, authenticated routes,
mobile/native adapters, live projections, or Folio reconciliation. Rollback is
limited to the new UI module, exports, localization keys, and this evidence.
