# G4-008/009-INCIDENT-TRANSPORT-032 evidence

## Delivered behavior

- Added additive `CreateIncident` and `UpdateIncident` RPCs to the existing
  `SupportService`.
- Both handlers require an authenticated access token, actor, and active
  session through the existing server boundary, then validate the command
  context and correlation identifiers.
- Incident commands require platform scope and the corresponding
  `support.incident.create` or `support.incident.update` role permission.
- Incident creation requires `commander_ref` to match the authenticated
  principal, bounds all lists and text fields, and parses every affected
  customer reference as a UUID.
- Create IDs are derived from the command ID so a retried command can reuse
  the existing `SupportPersistence` idempotency record rather than creating a
  second incident.
- Updates require `CommandContext.expected_aggregate_version`, use the
  existing `Incident::transition` lifecycle rules, and delegate optimistic
  concurrency/idempotency to `SupportPersistence::update_incident`.
- Responses expose only a safe summary DTO: identity, status, commander,
  leads, affected services/customers, and aggregate version. Timeline,
  broadcasts, mitigation, root cause, corrective actions, credentials,
  tokens, and diagnostic payloads do not cross this transport.
- Create and transition attempts are recorded with actor, purpose, and the
  validated command correlation ID through the existing audit sink.

## Verification

From `erpx/`:

```text
cargo fmt -p erpx-server -- --check
PASS

cargo test -p erpx-server
PASS — 42 passed, 0 failed

cargo clippy -p erpx-server --all-targets -- -D warnings
PASS

cargo fmt --all --check
PASS

cargo clippy --workspace --all-targets -- -D warnings
PASS

cargo test --workspace
PASS — all executed tests passed; database-backed suites remained explicitly ignored

bun run lint
PASS — 0 errors, 4 pre-existing warnings in `infra/scripts/start-n8n.js`

bunx tsc --noEmit
PASS
```

The server test build compiled the additive `erpx-proto` contract and the
generated Tonic service implementation.

The Folio development server remained operational on port 3004; `/operations`
and `/platform` both returned the expected `307` redirect.

## Files changed

- `crates/proto/proto/erpx/platform/v1/platform.proto`
- `apps/server/src/main.rs`

## Contract and migration effects

- Protobuf changes are additive: two messages, two response messages, one safe
  DTO, and two `SupportService` RPCs. Existing field numbers and services are
  unchanged.
- No application, domain, infrastructure, worker, UI, or migration files
  were changed by this packet.
- Runtime persistence uses the already-existing control-plane incidents table
  and `SupportPersistence` methods. No schema change is required.

## Unfinished work

- This packet did not add the incident list RPC, projection, worker
  orchestration, broadcast transport, or dedicated UI. A separately
  integrated `GetIncident` detail query and the later G4-008 incident-list
  packet now provide the authenticated safe summary read boundaries.
- The existing SupportPersistence incident methods remain the bounded
  persistence seam; full command/outbox integration is outside this packet.
- This evidence does not claim full G4 or G9 completion.

## Security concerns and integration order

- Keep the transport summary DTO narrow until a separate disclosure policy
  authorizes detailed incident evidence.
- Integrate after the existing authenticated command boundary and support
  permission data are deployed; then independently verify role revocation,
  audit visibility, and persistence idempotency against a disposable control
  database.

## Rollback

Remove only the additive incident messages/RPCs, server handlers/helpers/state
handle, tests, and this evidence directory. Existing ticket, capability,
domain, persistence, migration, worker, UI, and Folio behavior is unaffected.
