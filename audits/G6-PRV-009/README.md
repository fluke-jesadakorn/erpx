# G6-PRV-009 — Holding registry pagination evidence

## Scope

This focused test independently exercises the control-plane holding registry
page used by Platform Operations. It does not claim the full Platform
Operations product or G6 completion.

## Delivered behavior

- Bounded holding registry pages reject zero and over-limit sizes.
- Rows are ordered by opaque holding ID and use an exclusive keyset cursor.
- All registry states are eligible for the platform operations read path.
- The real test applies clean control write/read migrations and verifies the
  first and second pages against physically separate control databases.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure --test holding_registry_page` — PASS:
  1 focused test is documented ignored without database URLs.
- Real disposable PostgreSQL run of
  `holding_registry_page_is_bounded_ordered_and_exclusive` — PASS: 1 passed
  using `erpx_holding_page_cw_20260725` and `erpx_holding_page_cr_20260725`;
  both databases were dropped afterward and cleanup returned no rows.

## Security and remaining work

The page returns control-plane registration metadata to the server-side
Platform Operations adapter only. The public RPC excludes database keys,
storage namespaces, credential references, and backup references. Remaining
work includes live UI consumption, detail/action adapters, platform audit
views, and authoritative command flows.

## Rollback

Remove the additive integration test and audit record; no production or holding
data was changed by the evidence run.
