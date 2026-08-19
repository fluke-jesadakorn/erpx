# G4-009 shared support UI evidence

The shared Dioxus UI crate now contains a transport-neutral support and
incident presentation contract for platform and holding support surfaces.

## Delivered behavior

- Registers support list/new/detail/access approval/platform access,
  diagnostics, sessions, and incident list/detail routes.
- Models the 8-hour diagnostic-read and 2-hour mutation/impersonation limits.
- Carries customer-approval, read-only, and represented-user impersonation
  metadata without capability secrets or tokens.
- Provides bilingual lifecycle labels and explicit loading, empty, denied,
  stale, read-only, conflict, offline, and error states.
- Allow-lists display fields and excludes internal notes, credentials, bearer
  material, and signed URLs.

## Independent verification

Run from `erpx/`:

| Command | Result |
|---|---|
| `cargo fmt -p erpx-ui -- --check` | PASS |
| `cargo test -p erpx-ui` | PASS; 38 tests |
| `cargo clippy -p erpx-ui --all-targets -- -D warnings` | PASS |

## Open work and rollback

The model is not yet wired to support transport DTOs, customer-approved
capability-bound diagnostics, incidents persistence, native target adapters,
or live support routes. Rollback is limited to the support UI module,
exports/localization keys, and this evidence.
