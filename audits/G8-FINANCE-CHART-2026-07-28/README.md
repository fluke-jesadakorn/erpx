# G8 Finance chart read consistency — 2026-07-28

## Scope

Finance chart reads now fail closed when a normalized account row's
`aggregate_version` does not match the authoritative chart header. The read
adapter exposes stale-row evidence for reconciliation instead of presenting a
mixed-version chart, while preserving holding/holding scope and the existing
SQLx write, outbox, projector, and rollback boundaries.

## Evidence

- Focused Finance chart persistence tests: 2 passed.
- Workspace format, warnings-denied Clippy, and non-ignored workspace tests
  passed after this packet was integrated.
- Live disposable Finance SQLx acceptance remains queued because the required
  database references were not configured in the session.

## Safety and rollback

The change is read-side and fail-closed. It does not rewrite chart or account
authoritative state. Reconciliation can repair the projection from the
authoritative chart/account stream; rollback is code-version rollback plus
replay from the durable outbox/checkpoint. No production state changed.
