# G12-UI-SUPPORT-INCIDENT-SURFACES-043

## Delivered behavior

- Added reusable shared Dioxus surfaces for every existing support route, the platform incident
  list, and the platform incident detail route.
- Added `SupportRouteTabs` and a typed `SupportIncidentSurface` dispatcher. Navigation callbacks
  only request route changes; they do not issue commands or grant access.
- Rendered explicit loading, empty, denied, stale, read-only, conflict, offline, and error state
  panels from the existing support and incident state machines. Stale/offline/conflict cached data
  remains visible only as read-only content.
- Added a second presentation redaction boundary for field values and incident list/detail values.
  Token, bearer, secret, credential, password, key, signed-URL, and unsafe route-segment values
  are omitted before rendering or becoming links.
- Added responsive support/incident cards, safe-field grids, incident chips, semantic headings,
  keyboard-focus styling, screen-reader labels, and Thai/English route/status/state labels through
  existing localization conventions.
- Mounted the surfaces in the console behind explicit route tabs. The console uses typed empty
  states until an authorized support/incident transport adapter is supplied; it does not fabricate
  records or expose mutation controls.

## Files changed

- `erpx/crates/ui/src/support_surfaces.rs`
- `erpx/crates/ui/src/lib.rs`
- `erpx/crates/ui/assets/ui.css`
- `erpx/apps/console/src/main.rs`
- `erpx/audits/G12-UI-SUPPORT-INCIDENT-SURFACES-043/README.md`

No domain, application, infrastructure, proto, server, worker, Folio, database, or migration
paths were changed by this packet.

## Verification

Run from `erpx/`:

| Command | Result |
|---|---|
| `cargo fmt -p erpx-ui -p erpx-console -- --check` | PASS |
| `cargo test -p erpx-ui -p erpx-console` | PASS; `erpx-ui` 48 tests and `erpx-console` 11 tests |
| `cargo check -p erpx-console --no-default-features --features web` | PASS |
| `cargo check -p erpx-console --no-default-features --features desktop` | PASS |
| `cargo clippy -p erpx-ui --all-targets -- -D warnings` | PASS |
| `git diff --check` | PASS |

The focused tests cover all 13 support/incident route templates, typed list/detail scaffold
selection, explicit stale/denied states, secret/token value omission, and unsafe incident-link
rejection.

## Integrated verification

The concurrent domain/support lint findings were repaired by their owners. The
required integrated wave subsequently passed:

- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

The only remaining lint output is four pre-existing Folio warnings in
`infra/scripts/start-n8n.js`; no UI or console errors remain.

## Residuals and security boundary

- This packet does not add gRPC transport, protobuf conversion, support-ticket queries, incident
  queries, notifications, or incident commands. The consuming application must convert an
  authorized response into the existing safe display models.
- Console support/incident routes intentionally render typed empty states until that transport
  adapter exists. This is a presentation scaffold, not evidence of a live support backend.
- Server authorization, customer approval, capability expiry/revocation, audit attribution, and
  tenant/platform scope remain authoritative outside this crate.
- Internal incident timeline, broadcasts, mitigation, root cause, corrective actions, evidence,
  credentials, and secrets are not represented or rendered.

## Integration order and rollback

1. Keep the server-authorized gRPC read boundary authoritative.
2. Convert safe transport DTOs into `SupportDisplayModel`, `IncidentListDisplayModel`, or
   `IncidentDisplayModel` at the application boundary.
3. Supply the corresponding explicit state to `SupportIncidentSurface` on the typed route.
4. Independently verify browser and native target routing before enabling live support/incident
   transport.

Rollback is limited to the five owned UI/console/evidence paths listed above.
