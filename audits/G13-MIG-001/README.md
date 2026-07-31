# G13-MIG-001 — Ordered migration run and cutover guard

## Objective

Provide a deterministic, fail-closed migration-run contract that prevents a
cutover caller from skipping reconciliation, rollback rehearsal, final-delta
import, or the Folio retention window.

## Delivered behavior

- `MigrationRun` validates a non-nil run ID, bounded cohort/namespace
  references, and an exact 64-character hexadecimal source-contract hash.
- `MigrationPhase` enumerates the required G13 order from source freeze through
  Folio retirement; `advance` permits only the immediate next phase.
- `CommandOwner` is derived from the phase. ERPX becomes the owner only after
  mutation freeze and final-delta import, and Folio cannot become retired while
  retention is incomplete.
- A retention window is explicit, cannot be scheduled before Folio is
  read-only, and cannot complete at the exact pre-expiry boundary.
- The contract is serializable without database URLs, credentials, storage
  namespaces, tokens, or source business payloads.

## Files changed

- `crates/migration/src/lib.rs`
- `audits/G13-MIG-001/README.md`
- `MASTER_PLAN.md`

No database schema, Folio route, external runtime, or command-owner flag was
changed. The state machine is an integration contract for the future durable
migrator and cutover adapter.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-migration` — PASS; 8 tests, 0 failures.
- Exact sequence, no-skip, ownership, invalid-hash, retention-boundary, and
  terminal-retirement assertions pass.

## Open requirements

Live Folio source-contract freezing, deterministic backfill adapters, physical
write/read projection import, shadow replay, context-by-context reconciliation,
cohort rollout, mutation-freeze orchestration, durable command-owner
persistence, rollback rehearsal against real databases, and final Folio write
retirement remain open. This packet must not be used as evidence that any
customer has been cut over.

## Rollback

Revert only the migration state-machine code, this audit, and its master-plan
entry. No database or runtime state was changed.
