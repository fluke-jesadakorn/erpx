# G4-008 incident read projection evidence

## Delivered

- Added a rebuildable control-read `erpx_control.incidents` projection table.
- Stored fields are limited to safe incident summary data and aggregate version.
- Projection upserts are monotonic by aggregate version and are idempotent for replayed events.
- Added a registered outbox projector with clear/rebuild support and unrelated-aggregate filtering.

## Verification

- `cargo check -p erpx-worker -p erpx-infrastructure` — PASS before the isolated concurrent billing packet entered its intermediate edit state.
- The control-read migration is included by `CONTROL_READ_MIGRATOR`.

## Residual scope

The incident command path still needs authoritative command/outbox composition,
server reads still use the control write repository, and broadcasts,
notifications, timeline/postmortem disclosure, and rendered/native UI remain
open.

## Rollback

Revert the read migration, repository projection methods, worker projector and
registration, and this evidence file. No write-side or Folio data was changed.
