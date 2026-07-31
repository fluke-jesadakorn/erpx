# G8-FIN-004-061 — Finance banking import and reconciliation domain

## Delivered behavior

Added the pure `erpx-domain::finance_banking` slice for a later SQLx-backed
Finance banking implementation:

- Company- or branch-bound `BankAccount` records with explicit holding and
  company bindings.
- `CommandContext` validation and tenant-scope containment for accounts,
  statement imports, and reconciliations. Platform, holding-only, subtree, and
  mismatched record scopes fail closed.
- `ImportedBankTransaction` records with private identity/content fields,
  deterministic account-scoped source identity keys, content digests, bounded
  integer minor-unit amounts, and no mutation methods.
- `BankStatementImport` records with bounded statement windows, deterministic
  account/source-reference idempotency keys, stable content digests, duplicate
  row rejection, account/currency binding, and immutable source facts.
- `BankImportBook` conflict semantics for first acceptance, exact replay,
  changed-content idempotency conflict, duplicate import IDs, and duplicate
  transaction identities across statements.
- `BankReconciliation` with version-checked, fail-closed lifecycle:
  `Draft → InProgress → Reconciled → Closed`, or
  `InProgress → Exception → InProgress`.
- Exact integer difference calculation with checked arithmetic. Reconciliation
  is accepted only at exactly zero; non-zero differences must be rejected or
  explicitly raised as an exception. An exact adjustment can bring a working
  reconciliation to zero. Closed records cannot be reused or mutated.
- No provider credentials, raw bank secrets, external calls, persistence, or
  journal posting logic.

## Files

- `crates/domain/src/finance_banking.rs`
- `crates/domain/src/lib.rs`
- `audits/G8-FIN-004-061/README.md`

## Tests and exact results

From `erpx/`:

```text
cargo fmt --all --check                         PASS — exit code 0, no diff
cargo test -p erpx-domain --lib                 PASS — exit code 0; 128 passed, 0 failed, 0 ignored
cargo clippy -p erpx-domain --all-targets -- -D warnings
                                                PASS — exit code 0, no warnings
```

The six banking tests cover cross-company scope rejection, idempotent replay
and changed-content conflict, duplicate transaction detection, exact zero
difference reconciliation, non-zero exception/adjustment handling, and
terminal-state behavior. The full domain suite also passed without failures.

## Contract and migration changes

The only contract change is registering the new public domain module from
`crates/domain/src/lib.rs`. No shared transport contract or protobuf changed.
No database or migration changes were made.

## Open work and security concerns

This packet does not complete Finance. The following remain open:

- SQLx control/tenant write and read persistence, unique constraints, RLS,
  optimistic-concurrency transactions, outbox events, and projection rebuilds.
- Bank statement parsers/import adapters, provider-specific authentication,
  raw-file/object handling, malware/content validation, and credential/key
  management. These must remain outside this pure module and must never place
  provider secrets in domain records.
- Durable import workers, retry/dead-letter handling, reconciliation matching,
  journal/GL posting handoff, cash-control integration, audit projection,
  approval/SOD enforcement, transport APIs, and Finance UI.
- Production backup/restore, observability, migration reconciliation, and
  end-to-end security/integration tests.

## Integration order

1. Review and preserve these pure-domain invariants at the SQLx repository
   boundary, including idempotency/content-digest and transaction-identity
   uniqueness in one database transaction.
2. Add tenant-scoped statement import adapters and durable import/outbox
   processing without exposing credentials to domain or client code.
3. Add authoritative matching and Finance journal handoff only after period,
   entitlement, RBAC, and segregation-of-duties checks are enforced at the
   write boundary.
4. Add reconciliation transport/read projections, audit evidence, worker
   retries, and UI after persistence and command authorization are verified.
