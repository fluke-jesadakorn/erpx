# G14-QA-G4G7-037 evidence

Independent QA reviewed the integrated G4 Support/Incidents and G7 Operations
workstreams against the execution contract on 2026-07-24.

## Result

Both gates remain partial and must not be marked complete.

Directly evidenced primitives include ticket lifecycle, support capability
hash-only storage and bounded revocation seams, the domain support intersection
evaluator, incident lifecycle and authenticated safe list/detail transport,
Operations aggregate contracts, holding write/read persistence, RLS, event
idempotency/checkpoints, and bounded exception persistence.

Missing or partial requirements include customer approval scope containment,
authoritative device/session resolution, production role/customer cascade
invocation, capability-bound diagnostics, incident projections/broadcasts/
notifications, universal support enforcement, Operations consumers and worker
projection scheduling, exception workflows/SLA/reassignment, authenticated
Operations routes, rendered/native UI, and Folio reconciliation.

## Independent checks

- `cargo test -p erpx-domain --lib` — PASS; 56 tests.
- `cargo test -p erpx-ui` — PASS; 42 tests.
- `cargo test -p erpx-server` — PASS; 42 tests.
- `cargo test -p erpx-infrastructure --lib` — PASS; 42 tests, 3 ignored at
  the time of review.
- `cargo test -p erpx-worker` — PASS; 14 tests, 1 ignored.
- Server clippy and `git diff --check` — PASS.

The later independent Main verification raised infrastructure unit coverage
to 43 tests and separately passed the five-test disposable support suite.
