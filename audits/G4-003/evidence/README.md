# G4-003 / G4-007 / G4-008 domain foundation

## Delivered behavior

- `SupportCapabilityBinding` binds a capability to the customer, agent, device,
  session, ticket, mode, normalized permission set, scope, and module.
- `SupportCapability::issue` returns a one-time 64-character hexadecimal bearer
  value containing 256 bits of OS-random material and stores only its private
  SHA-256 digest. The bearer value has a redacted `Debug` implementation and is
  not serializable.
- Capability validity is half-open (`issued_at <= at < expires_at`), bounded by
  the mode maximum, and checked against the complete immutable binding.
- Reissue creates a new capability and secret, preserves the original expiry,
  records `reissued_from`, and refuses inactive capabilities.
- Revocation records an immutable reason and timestamp. Repeated revocation,
  expired capabilities, wrong bindings, and wrong secrets fail closed.
- `DiagnosticRedactionPolicy` is an allowlist for versions, projection lag,
  failed jobs, integration errors, worker/backup health, correlation IDs, and
  already-redacted configuration. Unknown fields—including credentials,
  secrets, private keys, payment instruments, tokens, connection strings, and
  unrelated business data—are omitted from `DiagnosticReport`.
- `Incident` models the required lifecycle:
  `investigating -> identified -> monitoring -> resolved ->
  postmortem_complete`. It records commander, leads, affected services and
  customers, timeline entries, broadcasts, mitigation, root cause, and
  corrective actions with completion tracking.

## Interfaces and persistence

- No shared contracts, protobufs, migrations, infrastructure, server, or UI
  files were changed.
- The identity crate's `TokenHash`/`SecretValue` types are not accessible from
  the pure domain crate without adding a dependency or changing shared
  contracts. A private SHA-256 implementation is therefore used locally over
  two UUIDv4 values (256 bits total); only the digest is held by the aggregate.
  This preserves the hash-only invariant without weakening the secret entropy.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain` — PASS, 34 passed, 0 failed.
- `cargo clippy -p erpx-domain --lib -- -D warnings -A clippy::collapsible-if -A clippy::too-many-arguments` — PASS for the domain library, including the new support code.
- The exact requested `cargo clippy -p erpx-domain --all-targets -- -D warnings` is blocked by existing out-of-scope `commercial.rs` issues: three denied lints in the library and a stale test call at `commercial.rs:925` that does not match the current request-based API. No out-of-scope file was modified to bypass these failures.

## Unfinished work

- SQLx persistence, outbox events, capability issuance/approval workflows,
  customer approval enforcement, support authorization intersection, and
  server interceptors remain integration work for later packets.
- Full diagnostic collection, incident notifications, SLA handling, and
  customer/support UI remain outside this domain-only packet.
- The private SHA-256 implementation should be replaced by the shared identity
  `TokenHash` only when a dependency-safe shared hashing boundary is approved.

## Security concerns

- The bearer value must be returned once by an adapter and never logged,
  serialized, or persisted. Persistence must store `secret_hash` only.
- UUIDv4 entropy relies on the platform RNG implementation used by the pinned
  `uuid` dependency. An integration adapter must not replace it with a
  deterministic source.
- Diagnostic producers must pass all fields through `DiagnosticRedactionPolicy`
  and must treat `redacted_configuration` as already sanitized.
- Capability revocation is modeled here but does not terminate a live session
  until the server/worker integration consumes the revocation event.

## Integration order

1. Add write-side capability/request persistence and atomic outbox events.
2. Bind customer approval and support session creation to this aggregate.
3. Enforce capability authentication at the authoritative server boundary,
   including agent role, session, module, billing, scope, and represented-user
   checks.
4. Project redacted diagnostics and incident state to platform/customer views.
5. Add worker expiry/revocation handling and end-to-end support tests.

## Rollback

This packet has no database or external-state effects. Rollback is limited to
reverting `crates/domain/src/support.rs` and this evidence file; existing ticket
behavior outside the new types remains unchanged.
