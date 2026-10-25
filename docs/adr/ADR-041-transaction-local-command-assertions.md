---
status: proposed
---

# Transaction-local command assertions

Concurrent hotel commands under SERIALIZABLE can conflict while inserting and deleting unrelated transient `command_assertions` rows. The owned PostgreSQL diagnostic identifies this shared guard relation, and a two-connection RED reproduces `40001` without modifying a hotel or money row. Keep command guard results in transaction-local PostgreSQL configuration behind the existing security-invoker SQL view and INSTEAD OF triggers; keep every original row in a baseline table. Each actual guard predicate still executes in its original statement, false/null/duplicate guards retain the original SQL constraints, and command-scoped deletion and savepoint rollback retain their behavior. This removes shared writes for new transient guards while keeping transaction isolation, Property locks, current authority checks, canonical receipt uniqueness, audit and money effects unchanged.

Transient guard state clears at commit or rollback and is private to its connection; it is not durable evidence. Receipts and audit remain durable evidence, with the original receipt insertion before business effects. Old baseline rows retain their existing visibility and administrative update/deletion; guard insertion in autocommit validates only that statement. Use logical DELETE for guard fixture cleanup because the interface is now a view. A global coordinator would serialize unrelated hotels, and reducing isolation or omitting guard predicates would change the command contract. Neither is required by this decision.

PostgreSQL documents [transaction-local set_config](https://www.postgresql.org/docs/18/functions-admin.html) and [SET LOCAL/savepoint behavior](https://www.postgresql.org/docs/18/sql-set.html). Migration 1039 implements the boundary; native and compiled acceptance remain pending. The [production plan](../production-plan.md) remains the sole active delivery checklist.
