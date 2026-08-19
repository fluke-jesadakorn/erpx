# G3-006 Customer 360 shared UI model evidence

The shared Dioxus UI crate now contains a transport-neutral Customer 360
display model for all ten required platform customer sections. It keeps
presentation state separate from server authorization and does not carry
tokens, holding credentials, or secret values.

## Delivered behavior

- Models overview, organization, contacts, commercial, entitlements,
  onboarding, health, support, access audit, and notes sections.
- Carries projection freshness and read-only metadata into the display model.
- Represents loading, empty, denied, stale, read-only, conflict, offline, and
  error states explicitly.
- Applies bounded safe field conversion/redaction before display.

## Independent verification

Run from `erpx/`:

| Command | Result |
|---|---|
| `cargo fmt -p erpx-ui -- --check` | PASS |
| `cargo test -p erpx-ui` | PASS; 26 tests |
| `cargo clippy -p erpx-ui --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS |
| `cargo check -p erpx-console --no-default-features --features web` | PASS |
| `cargo check -p erpx-console --no-default-features --features desktop` | PASS |

## Open work and rollback

Native Dioxus target adapters, transport bindings, authenticated deep-link
rendering, pagination, and the full platform/holding product surfaces remain
open. Rollback removes only the shared model/export additions.
