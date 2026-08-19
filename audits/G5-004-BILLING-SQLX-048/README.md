# G5-004 Billing/entitlement SQLx command adoption — stopped handoff

Status: **stopped at a coherent infrastructure command seam** at the user's
request. Shared plans and `erpx/EXECUTION_LEDGER.md` were not edited.

## Delivered behavior

`SqlxCommercialStore` now exposes transactional command methods for:

- `apply_provider_billing_event` — accepts a typed `BillingPolicyCommand` and
  verified `ProviderBillingEvent`; it never accepts a requested billing state.
- `apply_provider_entitlement_update` — accepts a typed provider entitlement
  snapshot and persists capacity/module activation watermark data.
- `advance_billing_policy` — advances only the system grace/read-only policy;
  payment recovery remains provider-authoritative.
- `allocate_entitlement_company` — applies company allocation capacity checks.
- `activate_entitlement_company` and `activate_entitlement_branch` — apply
  time-aware company/branch activation checks.

Each command uses one control-write transaction for:

1. request-hash idempotency reservation or receipt replay;
2. a transaction-scoped advisory lock for first-write and concurrent aggregate
   serialization;
3. locked aggregate read and typed domain transition;
4. snapshot persistence with optimistic version checks;
5. a redacted commercial audit payload; and
6. typed receipt completion.

Provider timestamps suppress equal/older billing and entitlement snapshots.
Provider event IDs are validated as required references. Idempotency hash
mismatches and in-progress conflicts return bounded `PersistenceError` values
without echoing keys, provider payloads, or database diagnostics.

The existing Stripe semantic applier now routes billing and entitlement events
through these command methods. The older low-level snapshot upserts remain for
compatibility but are no longer used by that semantic runtime path.

## Files changed

- `erpx/crates/infrastructure/src/commercial.rs`
- `erpx/audits/G5-004-BILLING-SQLX-048/README.md`

No migration, domain, protobuf, server, worker, identity, access, support,
customer, operations, UI, Folio, plan, or ledger file was changed by this
packet.

## Database effects and schema limitation

No migration was added. The implementation uses the existing
`erpx_control.billing_lifecycles`,
`erpx_control.entitlement_allocation_books`,
`erpx_control.idempotency_keys`, and `erpx_control.audit_events` tables.
Provider ordering metadata remains in the existing aggregate JSON payload,
as established by the current schema. The current schema has no dedicated
commercial command/audit event table and no provider-event foreign key, so
provider event identity is retained in the lifecycle/book payload and audit
payload rather than in new typed columns. A later migration may normalize that
metadata only after the shared contract and reconciliation design are frozen.

## Exact verification

Run from `erpx/`:

- `cargo fmt --all --check` — **PASS**.
- `cargo check -p erpx-infrastructure` — **PASS**, with seven unrelated
  existing warnings in `support_persistence.rs` for unintegrated attachment
  symbols.
- `cargo test -p erpx-infrastructure --lib` — **PASS: 54 passed, 0 failed,
  4 ignored**. The ignored tests require disposable PostgreSQL control
  databases.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` —
  **BLOCKED** by seven unrelated `support_persistence.rs` warnings/errors:
  unused `HoldingScope`, unused attachment imports, and unused attachment
  migration/page types.
- Auxiliary `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings
  -A unused-imports -A dead-code` — **PASS**; no other clippy findings.
- `git diff --check` — **PASS**.

No disposable PostgreSQL command-adoption test was run in this stopped
handoff. The existing ignored tests cover the prior Stripe inbox/persistence
seam; the new command methods still require disposable control-database
coverage before the packet can be considered integrated.

## Residual work

- Add disposable PostgreSQL tests for atomic rollback, duplicate/replay receipt
  rehydration, same-key hash mismatch, concurrent first writes, stale provider
  events, exact seven-day read-only entry, payment recovery, allocation limits,
  activation expiry, and audit atomicity.
- Add durable provider-event reconciliation and a dedicated normalized provider
  event/audit schema if required by the frozen contract.
- Adopt `advance_billing_policy` in the worker and adopt all allocation/
  activation commands in the authoritative application/server boundary.
- Remove or restrict production access to the compatibility low-level upserts
  after callers migrate.
- Add entitlement/read-model projection, full billing reconciliation,
  customer-facing billing state, and runtime read-only enforcement.

## Security concerns

- Signature verification and provider authenticity remain transport/provider
  responsibilities; these methods consume already-verified typed provider
  values and do not verify raw Stripe requests.
- Application authorization, entitlement ownership, segregation of duties,
  and actor/session validation are not implemented in this infrastructure-only
  packet.
- No client-settable billing state was added. Provider event identity,
  lifecycle transitions, capacity constraints, activation windows, and the
  seven-day policy remain domain-controlled.
- Audit payloads contain only bounded command/provider identifiers and state
  facts; raw webhook payloads, request keys, credentials, and payment
  instruments are excluded.

## Integration order

1. Repair the unrelated attachment warnings and keep the strict workspace gate
   green.
2. Run disposable PostgreSQL command-transaction tests, including concurrent
   and rollback cases.
3. Integrate application authorization and audit context at the command owner.
4. Wire the worker's policy command and provider reconciliation through these
   methods.
5. Add read-model projection and billing/entitlement runtime enforcement.
6. Independently run the affected integration wave before recording the packet
   in the shared ledger.

## Rollback

No database rollback is required because no migration or runtime data change
was introduced. Revert the additions to
`erpx/crates/infrastructure/src/commercial.rs` and remove this evidence
directory. Preserve the pre-existing Stripe inbox/catalog persistence and
leave shared plans and the execution ledger unchanged.
