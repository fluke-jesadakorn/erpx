# G5-004 Billing command adoption — stopped handoff

Status: **stopped at the domain command-contract boundary** after the user
interrupted the packet. This handoff does not claim runtime command adoption.

## Delivered behavior

- Added typed `ProviderBillingEvent` and `ProviderEntitlementUpdate` values.
  They carry provider events and desired capacity/activation facts, not a
  client-settable `BillingState`.
- Added `CommercialCommandContext`, typed command request values, typed
  command results, and replay receipt values.
- Added provider billing-event application with equal/older timestamp replay
  rejection and provider watermark recording.
- Added worker/system payment-policy advancement. `PastDue` starts grace at
  the original failure timestamp and the exact seven-day boundary enters
  `ReadOnly`; recovery remains a provider event.
- Added provider entitlement update application, capacity enforcement, and
  time-aware company/branch activation constraints.
- Added read-only policy and command-context redaction tests. Idempotency is
  represented by the typed command context/receipt contract, but the SQLx
  execution path was not completed in this interrupted handoff.

## Changed files

- `erpx/crates/domain/src/commercial.rs`
  - Domain command contracts, provider-only lifecycle methods, policy
    advancement, activation checks, and focused tests.
- `erpx/crates/infrastructure/src/commercial.rs`
  - No semantic command changes were left in this stopped packet; the file was
    normalized with rustfmt after partial wiring was removed.
- `erpx/audits/G5-004-BILLING-COMMANDS-046/evidence/README.md`

No migration, transport, server, worker, UI, plan, ledger, or runtime data was
changed.

## Exact verification

Run from `erpx/`:

- `rustfmt --edition 2024 --check crates/domain/src/commercial.rs crates/infrastructure/src/commercial.rs` — PASS.
- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain --lib` — PASS: 80 passed, 0 failed, 0 ignored.
- `cargo test -p erpx-infrastructure --lib` — PASS: 54 passed, 0 failed, 4 ignored. Ignored tests require disposable PostgreSQL control databases.
- `cargo clippy -p erpx-domain -p erpx-infrastructure --all-targets -- -D warnings` — PASS.

No disposable PostgreSQL command-adoption test was run because the SQLx
command methods were not completed.

## Residual work

- Implement `SqlxCommercialStore` command methods using one control-write
  transaction for idempotency reservation, aggregate locking, state update,
  and receipt completion.
- Route Stripe semantic application through those methods and add the
  system-worker grace/read-only command.
- Add SQLx tests for duplicate/out-of-order provider events, recovery,
  exact grace boundaries, allocation overruns, activation expiry, and replay
  receipt rehydration.
- Adopt the command seams in application/server/worker composition and remove
  production use of the low-level lifecycle/book upserts. Those paths were
  outside this transport-neutral packet.
- Add entitlement/read-model projection, billing reconciliation, and full
  runtime read-only capability enforcement.

## Security concerns

- Stripe signature verification remains the transport responsibility and is
  not repeated in the pure domain crate.
- The existing low-level `upsert_billing_lifecycle`,
  `upsert_entitlement_book`, `BillingLifecycle::transition`, and
  `EntitlementAllocationBook::set_active` APIs still exist; production callers
  can bypass the new domain command boundary until adoption is completed.
- No client-side billing-state setter was added, but no application-level
  authorization, entitlement, segregation-of-duties, or audit enforcement was
  added by this stopped packet.
- Command context `Debug` output redacts the idempotency key. No provider
  payload or secret is included in the new command receipt types.

## Integration order

1. Implement the SQLx transactional command service in
   `crates/infrastructure/src/commercial.rs`.
2. Independently verify PostgreSQL idempotency, optimistic locking, and
   out-of-order provider behavior.
3. Switch the existing semantic applier and worker policy to the command
   service.
4. Integrate application/server/worker authorization and audit boundaries.
5. Run affected integration tests, then update the shared execution ledger in
   the main-agent integration wave.

## Rollback

No database or external-state rollback is required. To remove this stopped
packet, revert only the additions to
`erpx/crates/domain/src/commercial.rs` and delete this evidence directory;
retain the pre-existing typed Stripe/catalog persistence in
`crates/infrastructure/src/commercial.rs`. No migration or command-owner flag
was changed.
