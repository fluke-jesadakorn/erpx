# G3-005-006-PROJECTOR-003

## Delivered behavior

- Added `CustomerAccessAuditSummaryProjection` as a named worker rebuild seam.
- Added explicit `clear` and `rebuild` operations.
- Rebuild reads only aggregate fields from authoritative write-side
  `erpx_control.global_read_audits`: customer ID, count, and latest access
  timestamp. Raw payloads and detailed attribution fields are never copied.
- Source rows are ordered by customer ID and only customer IDs already present
  in the control read customer projection are upserted, avoiding read-side FK
  failures for not-yet-projected customers.
- Read-side upserts are idempotent and preserve `updated_at` when aggregate data
  is unchanged.
- Customer-visible audit semantics are explicit: current global-read audit
  records are projected with `customer_visible = true`.
- No notes source or notes projector was added. The physical test verifies that
  notes remain empty rather than being fabricated.

## Files

- `apps/worker/src/lib.rs`
- `apps/worker/tests/customer_access_audit.rs`
- `crates/infrastructure/src/repositories.rs`
- `audits/G3-005-006-PROJECTOR-003/evidence/README.md`

No migration, domain, application, transport, UI, identity, commercial,
provisioning, environment, or Folio files were changed by this packet.

## Verification

Commands ran from `erpx/`:

```text
rustfmt --edition 2024 apps/worker/src/lib.rs apps/worker/tests/customer_access_audit.rs crates/infrastructure/src/repositories.rs
PASS

cargo check -p erpx-worker --tests
PASS

cargo test -p erpx-worker --lib
PASS: 2 passed, 0 failed, 1 ignored

cargo test -p erpx-worker --tests --no-run
PASS: worker unit and integration targets compiled

cargo clippy -p erpx-worker -p erpx-infrastructure --all-targets -- -D warnings
PASS

cargo test -p erpx-worker --test customer_access_audit
PASS: 0 passed, 0 failed, 1 ignored without database URLs

ERPX_TEST_CONTROL_WRITE_DATABASE_URL='postgresql:///erpx_g3005006_write_20260724' ERPX_TEST_CONTROL_READ_DATABASE_URL='postgresql:///erpx_g3005006_read_20260724' cargo test -p erpx-worker --test customer_access_audit -- --ignored --nocapture
PASS: 1 passed, 0 failed
```

The physical test used two explicit disposable local databases and both were
dropped after completion. It covers authoritative write-side audit insertion,
read-side projection, orphan-customer filtering, rebuild repeatability,
idempotent upsert timestamps, clear behavior, read/write table isolation, and
empty notes behavior.

## Security and isolation review

- The authoritative query uses `self.pools.write` only for `SELECT` from
  `global_read_audits`.
- All summary deletes and upserts use `self.pools.read`; no write-side table is
  mutated by the projector.
- The projection does not select or persist raw audit payloads, actor identity,
  scope, purpose, or correlation IDs.
- Errors crossing the worker seam are fixed, bounded, redacted messages and do
  not include database URLs, SQL, payloads, or secrets.
- Customer IDs without a read projection are skipped rather than inserted into
  a foreign-keyed read model.

## Unfinished work

- A scheduler/composition root still needs to invoke this rebuild at the chosen
  operational cadence or during read-database replacement.
- No authoritative note aggregate exists in the inspected write-side model, so
  notes remain unimplemented and empty.

## Integration order and rollback

1. Integrate the repository helper with the worker deployment.
2. Run the audit-summary rebuild after Customer 360 customer projection
   backfill and during replacement read-database rebuilds.
3. Expose the already-typed read method only after global-read authorization and
   customer visibility policy are wired at transport/application boundaries.

Rollback removes only the worker projection seam, repository read-model
helpers, focused test, and this evidence. It leaves authoritative write-side
global audits, customer projections, notes state, and all migrations intact.
