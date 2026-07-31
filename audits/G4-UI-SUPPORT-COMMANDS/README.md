# G4 Support and incident command surface

## Scope

The console Support and Incident routes now expose four bounded, authenticated
commands through the existing `SupportService` boundary:

- create a customer ticket;
- transition a ticket with an expected aggregate version;
- create an incident; and
- transition an incident with an expected aggregate version.

The shared Support presentation remains a redacted read model. The command
panel is a console adapter and does not render ticket bodies, replies, internal
notes, capability secrets, incident timelines, or raw RPC errors.

## Authoritative path

- `apps/console/src/main.rs` validates UUIDs, bounded labels/descriptions,
  allow-listed scalar inputs, CSV cardinality, and positive versions.
- The authenticated session supplies the actor and session identity. Incident
  commander identity is replaced with that authenticated principal; callers
  cannot forge it through form fields.
- The console sends a platform-scoped `CommandContext` with a support audit
  module, purpose, correlation ID, idempotency key, and expected version.
- `apps/server/src/main.rs` remains the authorization and command authority;
  it enforces Support permissions, MFA/capability policy, scope, optimistic
  concurrency, and tenant/platform routing before calling the existing
  `AuthoritativeSupportService`.
- The existing application and SQLx support paths persist source state,
  aggregate snapshots, idempotency receipts, append-only outbox events, and
  projection/checkpoint data. Worker/projector and reconciliation behavior
  remain the source of truth; the console refreshes the authenticated read
  resources after a successful command.

## Bounds and rollback

Input and receipt scalars are bounded and control-character rejected. The
client receipt contains only resource kind, ID, status, version, and
correlation metadata. Server-side idempotency and expected-version checks are
the retry and rollback boundary; read projections can be refreshed or
replayed without deleting authoritative source rows. No customer directory,
production command owner, Folio write path, or cutover state was changed.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-console --offline --no-fail-fast --quiet` — PASS,
  102 tests.
- `cargo clippy -p erpx-console --all-targets --offline -- -D warnings` —
  PASS.
- Console web, desktop, mobile, iOS, and Android target checks — PASS.

The local UI/API seam is complete for this packet. Live disposable database,
external support integration, production rollback, and customer activation
evidence remain governed by the coordinator acceptance matrix.
