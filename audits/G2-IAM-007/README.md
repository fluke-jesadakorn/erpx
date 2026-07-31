# G2-IAM-007 — Durable recovery approval receipts

## Objective

Make final Platform Root/Holding Owner recovery approval evidence durable and
replay-safe on the control-plane write database.

## Delivered behavior

- The existing `role_recovery_review_receipts` table is now used by the
  authoritative server recovery path.
- Receipt hashes bind the complete recovery request, assignment version, and
  decision; reason and ticket reference remain hash-only in storage.
- Transaction-scoped advisory locking serializes review-version allocation for
  one recovery request.
- Reusing an idempotency key with changed request material returns an
  idempotency conflict. Reusing the exact material returns the original
  receipt version.
- The receipt records target and scope, requester, independently authorized
  approver/reviewer, actor, command idempotency key, and correlation ID.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-server -p erpx-infrastructure` — PASS.
- Existing control migration `0015_role_review_persistence.sql` supplies the
  durable receipt table; no new schema or credential material was introduced.

## Nonclaims

This packet does not claim a pre-authorization artifact for ordinary role
renewal, notification provider delivery policy, or G2-G14 completion.

## Coordinator continuation — deterministic recovery retry repair — 2026-07-27

The authoritative application recovery service now rehydrates an existing
assignment whose ID equals the recovery request ID before evaluating a retry.
It validates the complete expected assignment against every other active
assignment and returns the existing immutable assignment without issuing a
second command. This closes the crash window between the role command/outbox
commit and the separate recovery-review receipt transaction; mismatched reuse
of the deterministic ID fails closed.

Evidence: `cargo fmt --all --check` and `cargo test -p erpx-application
--offline` — PASS, including
`recovery_retry_rehydrates_committed_assignment_without_second_command`.
