# `migrate.manufacturing.production-lots` — cutover spec

Status: **native cutover complete; acceptance deferred.** No new writer code is
required: the blocking consumer landed before this spec was written.

## Blocker resolution

The recorded blocker required `migrate.manufacturing.order-material-lifecycle`
to consume `erpx_server_manufacturing::public::lots` before the TypeScript lot
writers could be deleted. That consumer exists and is `migrated` at
`3627d7986a0d41486911b2db18d8f2dba55d6c26` (same SHA implemented and pushed):

- `src/composition/src/manufacturing_order_commands.rs` imports
  `public::lots::{plan_material_lots, post_material_lots, plan_output_lots,
  post_output_lots, ...}` and calls them on the issue/pass-QC paths.
- `src/composition/src/manufacturing_order_http.rs` mounts the command router;
  `private_development_api.rs` exposes `/manufacturing/orders/:id/:action`.
- The production route sends `issue`, `pass-qc`, `cancel` and `scrap` to that
  native command and fails closed with 503 when it is unavailable; there is no
  TypeScript lot-writer fallback.

## Four cutover layers

1. **Owner package** — `packages/server-manufacturing/src/{domain,application,adapters,public}/lots.rs`
   (producer at `151c7daa379ae71ea9dbd5816717f4e5f4ada32b`).
2. **Native client and route** —
   `packages/feature-manufacturing/src/routes/api/v1/holdings/[holdingSlug]/production/route.ts`
   POST dispatches the lifecycle actions to the native axum order command.
3. **Presenter/consumer** — composition `manufacturing_order_commands.rs`
   consumes `public::lots`; no workplace presenter reads the deleted sources.
4. **Retirement** — `packages/server-inventory/src/{adapters,public}/manufacturing-lot-sources.ts`
   and `packages/server-manufacturing/src/adapters/lots.ts` are deleted at
   `3627d7986a`. No import of either module remains in `packages`, `apps` or
   `tests` (the only `manufacturing-lot-sources.ts` left is the test oracle
   under `tests/support/manufacturing/legacy-production/`).

## KPI evidence

`3627d7986a` recorded `retirement:ts-writes` inventory **54 → 48** and
manufacturing **26 → 23** (nine writers removed). The later owner-wave cutover
`43ffc6a62c43bf89428606c6cb880370b17e0646` took the global TS write KPI to 0;
at current HEAD `f0f2eb8b8d07c0a2235d584b386fd00c5df3b54a` both owner counts
are 0.

## Retained legacy

- `packages/server-manufacturing/src/application/production.ts` — fail-closed
  branch for the remaining non-native actions; removal task `retire.manufacturing`.
- `packages/server-manufacturing/src/application/lots.ts` and `domain/lots.ts` —
  retained planning-lane lot oracle; removal task `retire.manufacturing`.
- `tests/support/manufacturing/legacy-production/manufacturing-lot-sources.ts` —
  test oracle only; removal task `retire.manufacturing`.

## Verification

At `f0f2eb8b8d07c0a2235d584b386fd00c5df3b54a` and again at the concurrent HEAD
`7431f1b388a0c0a46702f2e018498c950dcaad2f`:

- `cargo check -p erpx-server-manufacturing --offline` — passed.
- `bunx tsc --noEmit -p tsconfig.json` — passed.
- `cargo test -p erpx-server-manufacturing --test manufacturing_lots --offline` —
  passed, 13/13.

`bunx tsc -p tests/config/tsconfig.json --noEmit` passed at `f0f2eb8b8` but at
`7431f1b3` reports one unrelated error in
`tests/unit/assets/native-assets-transport.test.ts:81` (`TS2783`, concurrent
Assets WIP); removing this spec file does not change that result.
`bun run structure:check` likewise fails at both SHAs on a pre-existing
`TEST_INVENTORY_MISMATCH` from other workers' unregistered test files (Finance,
POS, Work Projection); this spec's path is not in that list.

Producer parity suite `manufacturing.rust-manufacturing-lots.unit`
(`tests/unit/manufacturing/rust-manufacturing-lots.test.rs`, 13 cases) is registered
in `tests/catalogue/suite-manifest.json` and the owner Cargo manifest.

## Next action for the lead

Bind `migrate.manufacturing.production-lots` to `retire.manufacturing` and record
it as integrated/migrated against `3627d7986a` (or as closed by the already
migrated `migrate.manufacturing.order-material-lifecycle`), with the checks
above. Keep `acceptance:false`; full acceptance stays deferred.
