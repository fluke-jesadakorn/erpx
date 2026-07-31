# G11 Quality projection reconciliation — 2026-07-28

## Scope

The Quality persistence slice now reconciles the authoritative inspection
state with its tenant read projection by inspection identity and source
version. It reports stale projections, projections ahead of the authoritative
version, missing rows, orphan rows, and version/count mismatches even when
aggregate counts happen to agree. The recovery path clears and rebuilds the
bounded read projection from the authoritative source and preserves the
existing append-only outbox and projector checkpoints.

## Evidence

- Focused Quality persistence tests: 4 passed.
- Workspace format, warnings-denied Clippy, and non-ignored workspace tests
  passed after this packet was integrated.
- The live SQLx acceptance remains queued because no disposable tenant
  database URL was configured in the session.

## Safety and rollback

The reconciliation query is read-only. Recovery is an explicit, scoped
projection rebuild; authoritative inspection rows and outbox history are not
deleted. A failed rebuild can be retried from the durable source/checkpoint,
and the pre-existing tenant read projection can be restored from the same
authoritative stream. No production activation or ownership change occurred.
