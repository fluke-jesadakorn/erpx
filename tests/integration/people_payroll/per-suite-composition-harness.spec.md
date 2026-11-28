# people_payroll per-suite composition harness — precise onboarding spec

Status: **spec only**. The shared per-suite composition harness is not ready for
people_payroll and is outside this worker's allowed paths
(`retire.people_payroll.people-payroll`, claim `run-20261123-e` / `worker-ppP2`).
Owner scope here is `modules/people-payroll/server`,
`modules/people-payroll/ui`, `tests/**`. The harness itself lives in
`scripts/testing/**` and is lead/shared-owned; this worker must not edit it, the
migration ledger, or push. Acceptance stays deferred.

## 1. Root cause (isolated, with evidence)

- Live-route proof **passed** on the fresh build (source SHA `acad72b15b161a52899b699c5c460a81cea3e59d`):
  `POST /api/v1/holdings/acme/people-payroll/.../schedule` returned
  `UNAUTHENTICATED` (session-gated), wrong-origin `ORIGIN_FORBIDDEN`, unknown
  holding `NOT_FOUND`. The native SaveSchedule mount is live.
- Re-running `people_payroll.core.integration` with the full 27-key native env
  at the same SHA still failed **22/22**. Adding keys did not help: the matrix
  is complete. The remaining fault is **cross-database**, not configuration:
  - The native transports resolve through
    `ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG`
    (`modules/people-payroll/server/src/adapters/native-people-payroll-command.ts`,
    `modules/people-payroll/ui/src/public/native-payroll-summary.ts`,
    `modules/people-payroll/ui/src/adapters/server/native-workforce.ts`)
    to the shared `erpx-server` private-api on loopback `:3198`, which is
    configured against the **shared dev DB**.
  - The lane seeds its fixture into its own leased/template DB (`H001_DB` slot,
    `tests/support/postgres-templates.ts`, `tests/support/runtime-env.ts`). The
    fixture company is **dynamic** per run:
    `expense-${randomUUID()}` in `fixtures/expenses/company.ts` (consumed via
    `tests/support/helpers/expense-fixture.ts`, used by
    `tests/integration/people_payroll/payroll-core.test.ts`).
  - That company does not exist in the dev DB behind `:3198`, so the native
    command route cannot resolve the actor/company and fails closed with
    `NATIVE_PEOPLE_PAYROLL_UNAVAILABLE` (503). Secondary in-lane error:
    `PACKAGE_DEPENDENCY_REQUIRED` (`modules/commercial/server/src/domain/package-dependencies.ts`,
    `modules/commercial/server/src/domain/packages.rs`) on the people package
    obligation preview, a downstream effect of the failed native authority path.

Fix: the lane's native transports must reach a composition instance bound to
the **same leased DB the lane seeds**, not the shared dev DB. That is exactly
what the per-suite harness epic provides; people_payroll must be onboarded onto
it. This is not fixable by adding more `ERPX_NATIVE_*` keys.

## 2. Harness state (lead/shared, `scripts/testing/**`)

- `scripts/testing/boot-suite-composition.ts`: `SUPPORTED_SUITES =
  ['inventory.transit.integration']`. It creates its **own** leased databases
  (`erpx_dev_rust_<hex>` control + h001/h002), migrates them, seeds a fixed
  company `probe-co`, picks a free loopback port >= 3310, boots
  `target/debug/erpx-server private-api --development-config ...`, waits for
  `/api/health` 200, prints an `ERPX_NATIVE_*` env matrix, and tears down.
  Current uncommitted WIP adds `--serve` (stdin EOF = teardown) and extra keys
  (`COMMERCE_WAVE2`, `COMMERCIAL_WAVE2`, `COMMERCE_OWNER_WRITE`,
  `FINANCE_OWNER_WRITE`, `WORKFLOW_WAVE3`).
- `scripts/testing/test.ts`: `COMPOSITION_SUITES = ['inventory.transit.integration']`.
  Opt-in only with `ERPX_TEST_NATIVE_COMPOSITION=1`; spawns the harness with
  `--serve`, parses `BOOT_SUITE_ENV_MATRIX_BEGIN/END`, injects the matrix into
  the vitest child env, then closes stdin for teardown. Suites not in
  `COMPOSITION_SUITES` fail closed (`NATIVE_COMPOSITION_SUITE_UNSUPPORTED`).

Conclusion: not ready for people_payroll — suite id is not registered, and the
harness provisions independent DBs rather than binding to the lane's DB.

## 3. Precise edits required (lead/shared — NOT this worker)

### 3.1 `scripts/testing/boot-suite-composition.ts`
1. Add people_payroll suite id(s) to `SUPPORTED_SUITES`. Primary:
   `people_payroll.core.integration` (file
   `tests/integration/people_payroll/payroll-core.test.ts`, postgres slots
   `CONTROL_DB`, `H001_DB`, `H001_READ_DB`). Then, for the full owner lane,
   `people_payroll.scheduled.integration`, `people_payroll.statutory.integration`,
   `people_payroll.expense.integration`, `people_payroll.leave-attachment-lock.integration`.
2. Add to `printEnvMatrix` the people_payroll key (shape validated by
   `native-people-payroll-command.ts` / `native-payroll-summary.ts`):
   ```
   ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG = {
     privateBaseUrl: baseUrl,            // http://127.0.0.1:<probe port>
     timeoutMs: 10000,
     holdings: [{ origin: 'https://acme.jesadakorn.com',
                  holdingSlug: 'acme', holdingId: 'h001',
                  companyId: <COMPANY> }] // must equal the seeded fixture company
   }
   ```
   Keep the transitive keys already in the matrix
   (`ERPX_NATIVE_COLLABORATION_CONFIG`, `*_COMMERCE_WAVE2_CONFIG`,
   `*_COMMERCIAL_WAVE2_CONFIG`, `*_COMMERCE_OWNER_WRITE_CONFIG`,
   `*_FINANCE_OWNER_WRITE_CONFIG`, `*_WORKFLOW_WAVE3_CONFIG`), and add
   `ERPX_NATIVE_ORGANIZATION_CONFIG` if the organization native read path is
   reached by these suites.
3. **Bind composition to the lane DB (the decisive change).** The harness must
   not create fresh DBs whose `probe-co` is unknown to the fixture. Preferred:
   `test.ts` passes the already-provisioned lane connection strings
   (`CONTROL_DB`, `H001_DB`, `H001_READ_DB`, `H001_FILES`) to the harness, which
   sets the development-config holding bindings to those URLs instead of
   creating/dropping its own. Teardown must then skip dropping lane-owned DBs
   (the lane runner owns their lifecycle).

### 3.2 `scripts/testing/test.ts`
1. Add the same people_payroll suite id(s) to `COMPOSITION_SUITES`, else the
   opt-in run fails closed.
2. Thread the lane DB URLs to the harness invocation (args or a private env
   block), and ensure `composition.env` overrides the 27-key shell matrix so
   the native transports target the spawned instance.
3. Preserve the existing fail-closed behavior (any selected suite outside
   `COMPOSITION_SUITES` aborts) — do not weaken it.

## 4. Design constraint: fixture company identity / boot order

The fixture company is generated inside the suite's `beforeAll`, **after** the
harness has already booted and printed its matrix. Two ways to make the
injected `companyId` match:

- **Option A (recommended, general):** bind composition to the lane's `H001_DB`
  (§3.1.3) and have the harness discover the company at boot by querying
  `organization_nodes` for `kind='company'` in the bound DB (or accept the value
  from `test.ts` after fixture seed). The native route then sees the exact
  dynamic `expense-<uuid>` company with no fixture coupling.
- **Option B (coupled, last resort):** make the people_payroll fixture seed a
  fixed company (e.g. `probe-co`) so the harness's current fixed seed matches.
  Only viable if the harness's fixed seed is kept; it hard-codes a shared
  harness detail into owner fixtures and is less reusable for other lanes.

Option B would be in this worker's scope (`fixtures/expenses/company.ts`,
`tests/support/helpers/expense-fixture.ts`) but should only be done after the
lead fixes the harness contract in §3.

## 5. In-scope readiness work (this worker, once §3 contract is fixed)

- Add/re-point people_payroll suite native-transport assertions
  (`tests/unit/people_payroll/native-payroll-summary-transport.test.ts`) to the
  harness-provided env, keeping fail-closed assertions
  (`NATIVE_PEOPLE_PAYROLL_UNAVAILABLE`, `...AUTHORITY_INVALID`,
  `...CONTEXT_CHANGED`).
- Migrate any remaining TS-oracle seeding in `tests/integration/people_payroll/**`
  to native-seeded fixtures as required by the retirement gate; remove only
  writers whose native counterpart is proven and whose TS callers are gone.
- Do **not** delete `modules/people-payroll/server/src/adapters/people-payroll.ts`
  (51 writes) or its barrels until native parity + no-live-TS-caller are proven
  and the owner KPI check strictly decreases.

## 6. Exit criteria / verification (lead runs composition + KPI)

```bash
ERPX_TEST_NATIVE_COMPOSITION=1 \
  bun scripts/testing/test.ts integration --suite=people_payroll.core.integration
# green 22/22; zero NATIVE_PEOPLE_PAYROLL_UNAVAILABLE / PACKAGE_DEPENDENCY_REQUIRED
cargo check -p erpx-people-payroll-server --offline
~/.bun/bin/bunx tsc --noEmit -p modules/people-payroll/ui/tsconfig.json
# retirement:ts-writes owner KPI strictly lower at deletion SHA
```

Non-goals for this worker: ledger/CLI mutations, push, `scripts/testing/**`,
root Cargo/lock, composition/runtime selection, schema/data/history deletion,
and any acceptance/certification flag change. Browser smoke remains
user-blocked; acceptance is deferred.
