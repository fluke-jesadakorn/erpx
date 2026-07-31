# ADR-003: No long-term dual writes

**Status:** Accepted  
**Decision:** During migration, Legacy changes may be captured for deterministic shadow comparison, but ERPX will not remain a second write target. After a cohort cutover, ERPX is the sole mutation owner; Legacy is read-only during its retention window and is then retired for that context.

**Rationale:** Long-term dual writes create divergent authority, ordering, and rollback ambiguity. Shadow replay and final delta import provide migration confidence without two live sources of truth.

**Consequences:** Each context needs a command-owner flag, source-contract freeze, deterministic IDs, final mutation freeze, delta import, rollback rehearsal, and read-only Legacy verification window.

**Evidence required:** cohort cutover logs, reconciliation/difference explanations, rollback rehearsal, command-owner enforcement, and proof that writes after cutover cannot reach Legacy.
