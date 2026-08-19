# G6/G12-UI-009 — Platform Operations console route surface

## Delivered behavior

- Integrated the existing `erpx_ui::platform` route registry into the Dioxus
  console shell and navigation.
- Rendered the exact seven Platform Operations routes:
  `/platform/holdings`, `/platform/holdings/:id`, `/platform/projections`,
  `/platform/backups`, `/platform/integrations`, `/platform/audit`, and
  `/platform/settings`.
- Kept holding detail navigation disabled until an authorized opaque holding ID
  is supplied; the holding list includes a blank, locally validated launcher for
  that typed route and no holding value is fabricated.
- Added a read-only, target-neutral surface for every typed route. With no
  data adapter connected, it explicitly renders loading, unavailable, denied,
  stale, read-only, and not-yet-connected states.
- Reused the existing Thai/English route metadata for labels and retained the
  existing Customer 360 and Support/Incident surfaces and session boundary.
- Added focused tests for exact route coverage, holding deep-link
  normalization/redaction, and safe non-ready/read-only copy.

## Exact verification

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-console --bin erpx-console` — PASS: 14 passed.
- `cargo clippy -p erpx-console --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

## Security and unfinished work

- The console does not render holding credentials, database keys, storage
  namespaces, readiness values, operational actions, or fabricated business
  data. Server authorization remains authoritative.
- The route registry rejects credential-bearing deep links and validates the
  holding segment before it can enter the typed route.
- Real Platform Operations protobuf adapters, projection freshness data,
  holding provisioning commands, backup/restore actions, and authoritative
  platform authorization wiring remain unfinished. The current surface is
  intentionally read-only until those contracts exist.

## Integration order and rollback

Integrate after G6-G12-UI-008, which provides the shared typed route registry.
The packet changes only `apps/console/src/main.rs` and this audit. Roll back by
reverting those two paths; the shared UI registry and existing Customer 360 and
Support/Incident routes remain intact.
