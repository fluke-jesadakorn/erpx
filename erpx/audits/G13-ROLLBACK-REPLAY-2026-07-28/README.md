# G13 durable rollback replay — 2026-07-28

## Scope

G13 target import recovery now restores a durable rollback snapshot when the
target/read projections are empty or only partially restored. Source fences,
idempotency keys, and append-only outbox replay remain authoritative, so a
retry does not duplicate imported identities or events. The recovery path is
bounded by tenant/holding scope and preserves the Folio bridge's read-only
legacy ownership boundary.

## Evidence

- G13 parallel compatibility/registration acceptance: 9 passed.
- G13 deployment/recovery acceptance: 4 passed.
- Focused G13 target-import tests and strict infrastructure checks passed.
- Disposable source/target/rollback SQLx execution remains queued because the
  required runtime database URLs were not configured.

## Safety and rollback

The implementation only makes recovery available behind the existing
authorization and runtime-reference gates. It does not activate a target,
change command ownership, retire Folio writes, or perform production cutover.
If replay fails, the durable rollback snapshot and source event fences remain
available for another idempotent recovery attempt.
