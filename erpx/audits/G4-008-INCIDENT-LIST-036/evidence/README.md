# G4-008-INCIDENT-LIST-036 evidence

## Delivered behavior

- Added additive `SupportService.ListIncidents` transport.
- Requires the existing authenticated active-session boundary, explicit
  platform scope, `support.incident.read`, correlated `QueryContext`, and a
  non-zero page size capped at 100.
- Supports an optional closed-set incident status filter.
- Records `incident_list_read:platform` before the database read. The shared
  audit event stores actor, purpose, and correlation independently; the fixed
  platform scope is explicit in the action because the existing audit schema
  has no separate scope column.
- Returns only the safe incident summary DTO: identity, lifecycle status,
  commander, leads, affected services/customers, and aggregate version.
- Persistence orders by `updated_at DESC, id DESC`, fetches at most
  `page_size + 1`, reports `has_more`, and never performs an unbounded query.
- Stable redacted persistence errors continue through the existing gRPC error
  boundary; invalid scope, authorization, context, status, and page inputs
  fail closed.

## Files changed

- `erpx/crates/infrastructure/src/support_persistence.rs`
- `erpx/crates/infrastructure/src/lib.rs`
- `erpx/crates/infrastructure/tests/support_persistence.rs`
- `erpx/crates/proto/proto/erpx/platform/v1/platform.proto`
- `erpx/apps/server/src/main.rs`
- This evidence file.

## Contract and database effects

- Appended `ListIncidentsRequest`, `ListIncidentsResponse`, and the
  `ListIncidents` RPC without renumbering existing protobuf fields or RPCs.
- No migration or schema change. The existing `incidents(status, updated_at
  DESC)` index is used by the bounded control-plane query.
- No tenant credentials, timeline, broadcast, mitigation, root cause,
  corrective action, secret, or diagnostic payload crosses the transport.

## Exact verification

All commands ran from `erpx/`:

```text
cargo fmt --all --check                                      PASS
cargo check -p erpx-server                                   PASS
cargo check -p erpx-infrastructure                           PASS
cargo test -p erpx-server                                    PASS: 42 passed
cargo test -p erpx-infrastructure --lib                      PASS: 42 passed, 3 ignored
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings PASS
cargo clippy -p erpx-server --all-targets -- -D warnings     PASS
cargo test -p erpx-infrastructure --test support_persistence -- --ignored --nocapture
                                                             PASS: 5 passed
```

The disposable PostgreSQL database used for the ignored suite was dropped
after the run. The focused test proves deterministic ordering, `has_more`,
status filtering, aggregate versions, and rejection of page sizes 0 and 101.

## Unfinished work

- Incident projections/read-model workers, broadcasts, notifications,
  timeline/postmortem disclosure, reconciliation, and full server/native UI
  rendering remain open.
- This packet does not complete G4 or G12 and does not claim incident worker
  orchestration or delivery.

## Security concerns

- Authorization and active-session validation remain server-boundary
  responsibilities and execute before audit/read work.
- Platform scope is required; holding/company/branch reads are denied.
- Audit persistence failure fails the request before the incident query.
- The DTO mapper remains the only boundary for incident payload exposure.

## Integration order

1. Keep the existing authenticated incident command/detail-read boundary and
   safe DTO mapper authoritative.
2. Bind generated `ListIncidents` responses to the shared UI list model.
3. Add projection/checkpoint workers and broadcast policy only in a separate
   dependency-ready packet.
4. Independently verify browser/native rendering and customer-visible audit
   views before G4 acceptance.

## Rollback

Remove the additive list messages/RPC, `IncidentPage` repository method,
focused test, server handler, export, and this evidence directory. Existing
incident create/update/detail-read behavior and the incidents table remain
unchanged.
