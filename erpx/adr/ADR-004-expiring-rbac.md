# ADR-004: Universal expiring RBAC

**Status:** Accepted  
**Decision:** Every ERPX role assignment has a bounded half-open validity window `[valid_from, valid_until)`, scope, functional domain, authority level, risk tier, grant/approval/review attribution, reason/reference, parent assignment, and immutable assignment version. Renewal creates a new version. No permanent assignment is representable through the command API.

**Rationale:** Expiry and parent cascade are security invariants, not UI reminders. A child cannot outlive or exceed the permissions/scope of its parent.

**Consequences:** The evaluator must check active identity/session, assignment/parent windows, scope containment, entitlement/module/billing state, permission, approval limit, SoD, and support capability where applicable. Expiry workers reassign work and emit notifications/review evidence.

**Evidence required:** exact boundary tests, parent revocation cascade, scope inheritance, high-risk two-person approval, renewal versioning, recovery, and authoritative write-side mutation tests.
