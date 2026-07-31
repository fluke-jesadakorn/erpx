# G4-009-INCIDENT-UI-035 evidence

## Delivered behavior

- Added typed, transport-neutral incident list and detail display models for
  `/platform/incidents` and `/platform/incidents/:id`.
- Added platform-only scope metadata with projection freshness and read-only state.
- Added strict mapping for the safe incident DTO fields: `id`, `status`, `commander_ref`,
  `leads`, `affected_services`, `affected_customer_accounts`, and `version`.
- Added lifecycle parsing and localized Thai/English labels for all five incident statuses,
  including terminal `postmortem_complete`.
- Added explicit loading, empty, ready, denied, stale, read-only, conflict, offline, and error
  state variants for both incident list and detail data.
- Invalid required records are rejected; suspicious repeated values are omitted at the display
  boundary. Internal timeline, broadcast, mitigation, root-cause, corrective-action, evidence,
  credential, and secret fields are not represented by the UI types.
- Preserved the existing support route registry and localization conventions.

## Files changed

- `erpx/crates/ui/src/support.rs`
- `erpx/crates/ui/src/lib.rs`
- `erpx/audits/G4-009-INCIDENT-UI-035/evidence/README.md`

`i18n.rs` was not changed because the required incident and shared state translation keys already
existed.

## Contract and database effects

- No protobuf, server, application, domain, infrastructure, worker, migration, or database changes.
- No transport client or native target implementation.
- Exported UI contracts: `IncidentDtoInput`, `IncidentScope`, `IncidentProjectionMetadata`,
  `IncidentListItem`, `IncidentListDisplayModel`, `IncidentDisplayModel`, `IncidentViewState`,
  `IncidentListState`, and `IncidentDetailState`.

## Verification

Commands run from `erpx/`:

```text
cargo fmt -p erpx-ui -- --check       PASS
cargo test -p erpx-ui                 PASS: 42 passed, 0 failed
cargo clippy -p erpx-ui --all-targets -- -D warnings
                                      PASS
```

Tests cover all incident lifecycle labels in both locales, both required routes and platform
scope, safe-field omission, invalid-record rejection, and all explicit state distinctions.

## Unfinished work

- Authenticated incident list/detail transport is now available, but projection
  workers, broadcast/notification orchestration, browser/native rendering, and
  G12 target completion remain open.
- This packet does not complete G4 or G12.

## Security concerns

- The display adapter has no secret-bearing fields and filters suspicious values from repeated
  DTO fields. Required identity/status failures fail closed.
- Authorization, platform scope enforcement, audit attribution, and command handling remain
  server responsibilities and are not duplicated or claimed here.

## Integration order

1. Keep the existing authenticated incident RPC and safe DTO boundary authoritative.
2. Convert generated transport DTOs into `IncidentDtoInput` at the application boundary.
3. Render `IncidentListState` on `/platform/incidents` and `IncidentDetailState` on
   `/platform/incidents/:id`.
4. Wire server-provided projection freshness and error states without collapsing explicit states.
5. Independently verify browser and native clients before broader G12 integration.

## Rollback

Remove only the incident models, exports, focused tests, and this evidence directory. Existing
support route/state behavior and all backend behavior remain unchanged.
