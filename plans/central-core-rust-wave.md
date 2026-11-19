# Core (ส่วนกลาง) Rust Axum Migration — deletion-first wave

## Context

Continue the Next.js API → Rust axum migration, focused on the **central core** with two user-set emphases:

1. **Scope** — core = S01 shared-execution + S02 authority + S03 commercial + S04 stock/accounting. **Skipped** (user decision): HR+Payroll+Benefits (`people_payroll`, `benefits`, `talent`, `learning`), `pos`, `pms`, "IT" = `agent` + `platform_administration` + `operations`.
2. **Deletion is immediate** — every slice = native Rust implementation + composition mount + route authority switch + **deletion of the replaced TS writer/branch/fallback in the same coherent slice**, with a strictly-decreasing `retirement:ts-writes` before/after recorded. No deferred cleanup queue. The KPI drop is the visible success metric.
3. **Writes first, reads second** — close all core TS writes to 0, then queue the read tail (`retire.reads.*`) as a follow-on pass.

Follows the standing protocol in `docs/production-plan.md#rust-migration-current`: lead claims via `bun scripts/testing/rust-migration-work.ts`, milestones push to `origin/codex/rust-backend-migration` only, 9-step slice procedure, acceptance stays deferred (A1).

### Measured baseline (HEAD `74dfbf8a6`, 2026-11-19)

Global write KPI: **1,950** (baseline 2,894 at `1399b44cf`). In-scope core holds **972** of it.

| Owner (crate) | TS writes | TS SELECTs | TS files | Ledger state |
| --- | --- | --- | --- | --- |
| `shared` | 0 | 0 | 6 | ✅ clean (audit only) |
| `finance` | 336 | 1,169 | 572 | blocked: goods-journal composition mount; bank-automation + due-draft jobs; live TS journal flows (`opening.ts`, `legacy-revenue.ts`, `journal-review.ts`) |
| `files` | 113 | 233 | 215 | native generic upload intake + scan-receipt authority missing; 15 residual TS files each still imported |
| `identity_access` | 110 | 325 | 457 | residual families: IAM migrations, GPO rules, account recovery, product SSO, people lifecycle, provisioning, preferences, cross-owner fact adapters |
| `workflow` | 95 | 67 | 183 | `ProjectionSideEffects` mount + `project_event`→`project_message` switch pending; approval-planning port incomplete (task branch only) |
| `billing` | 73 | 62 | 55 | transport conformance gap (cost cutover reverted); cross-Holding platform-session decision open |
| `organization` | 55 | 231 | 266 | job-principal / native-draft-authority decision pending; 15 People-change writers |
| `work_projection` | 41 | 80 | 221 | **no native calendar exists** — must build full calendar vertical; nav/summary/CSV need flow facts |
| `commerce` | 35 | 298 | 255 | native sales-order goods/mixed fulfillment + route switch; `sales-delivery` blocked on service-branch retirement evidence |
| `inventory` | 31 | 285 | 169 | transit/count composition mounts pending; executive-reorder oracle residual |
| `runtime` | 31 | 47 | 165 | executive reports/generation (16 writes) have **no native producer**; supervisor 7/7 + PG 8/8 already native |
| `records` | 27 | 36 | 103 | native record module + mention/suggestion/employee-request/request-AI writers pending |
| `commercial` | 12 | 31 | 61 | `purchase-command-boundary` downgraded — lacks retirement check + covering retire task |
| `manage_erpx` | 7 | 21 | 60 | blocked: composition has no source allocating onboarding slots |
| `customer_authority` | 5 | 46 | 54 | portal-tail blocked on portal-support/customer-360 split decision |

**Target end state:** core writes 972 → **0**; global KPI 1,950 → **~978** (all residual = skipped owners). Then read-tail pass reduces the in-scope TS to retained `public/native-*` transport + composition only.

## Approach

Deletion-first waves, biggest-keystone-first inside each. Every slice: implement Rust → mount in `src/composition/` (lead) → switch route authority (native primary, TS fallback branch removed) → delete replaced TS → record `retirement:ts-writes` before/after + register `retire.<owner>` binding per the mandatory rule.

### Wave 0 — quick wins (immediate visible delta, minimal Rust)

- [ ] `retire.unreachable-ts-cleanup`: rebuild the root-complete reachability audit (current `reachability.mjs` omits worker/script/test roots — the recorded correction), then delete the **81 dead TS modules** and their `package.json` export keys. KPI 0 → 0, but −81 files in one pushed milestone.
- [ ] `commercial.purchase-command-boundary` retirement: create the bounded retirement task (exact owner/feature-route/composition/test paths), run the native-seeded KPI delta, close, restore `migrated`.
- [ ] Ledger hygiene: convert the two "downgraded" packets (commercial, platform_administration — latter out of scope now, record as skipped-residual) to consistent states so counts are honest.

### Wave 1 — close the small core owners

- [ ] `manage_erpx` (7): lead adds the onboarding-slot allocation source to composition (OnboardingCommandResource write/read/files bindings) → mount `onboard_company`/`reserve_onboarding` → delete TS onboarding writers.
- [ ] `customer_authority` (5): decide portal-support/customer-360 split (lead decision, consumer paths in scope owners only) → close `retire.customer_authority.portal-tail` at 0.
- [ ] `records` (27): extend native record module; port mention/suggestion/employee-request/request-AI writers; own the workplace write routes in the same slice; delete.
- Milestone push M-A: record KPI (expect global ≈ 1,950 − 7 − 5 − 27 − 12 = ~1,900).

### Wave 2 — decision-blocked mids

- [ ] `work_projection` calendar vertical (41): build native calendar (domain/application/adapter for `calendar_appointments`/`series`/`announcements` — currently **zero** native references) + composition mount + route switch; then close `retire.work_projection.calendar-commands-facts` at 0. Also complete nav/summary/CSV flow facts (title/detail/status/dueAt/blocker/flow).
- [ ] `organization` (55): lead decision — job principal / native draft authority → switch People changes native-only → delete 15 writers → then remaining 40 family-by-family.
- [ ] `billing` (73): fix transport conformance first (root-cause the reverted cost cutover `f0aa0dfcd`), then cost cutover, then approve/void + target/retry; resolve cross-Holding platform-session decision (a/b) with the user.
- [ ] `commerce` (35): native sales-order goods/mixed fulfillment (`fulfillment.rs` + `order_http`), switch workplace route to native-only, delete TS `fulfillOrder` goods branch + public export; purchase receipts same pattern; close `retire.commerce.commerce` in command-level packets (save/return/invoice/settlement).
- [ ] `inventory` (31): extend composition with `inventory_transit_commands.rs` + `inventory_count_commands.rs` mounts, compose with current port set, switch stock-counts route; close `retire.inventory.commerce-stock-owner-effects` + executive-reorder-oracle residual.
- Milestone push M-B: KPI (expect ≈ 1,900 − 41 − 55 − 73 − 35 − 31 = ~1,665).

### Wave 3 — the big three

- [ ] `workflow` (95): mount `ProjectionSideEffects` in `src/composition/src/workflow_projection_worker.rs`; switch `project_event`→`project_message` for customer-notice first, add each family to `owned_predicate` as its owner drain goes native; build approval-planning vertical (expense/leave/overtime branches) and switch cross-owner TS callers (records in-scope; operations/legal/people_payroll callers repointed at transport layer only). Close `retire.workflow` + `retire.workflow.approval-planning`.
- [ ] `files` (113): implement native generic upload intake + scan-receipt authority from `inventory.files`; switch the live importers (support-portal, finance attachment reads, customer-portal — consumer-side transport switches); close `retire.files.service-repository` (55) + `attachments/migration.ts` backfill/reconcile (24, dev-only consumers) + rest.
- [ ] `identity_access` (110): family-by-family — IAM migrations → GPO rules → account recovery → product SSO → people lifecycle → technology/provisioning → preferences → cross-owner fact adapters; each family: native port + mount + switch + delete. Close `retire.identity_access.residual-writers` at 0.
- [ ] `runtime` (31): build the native executive reports/generation producer (TS `executive/evidence.ts` is a 3,121-line tree — port as a bounded vertical behind the existing native supervisor), switch the feature-apps executive route + job projector, delete. First run `audit.runtime` reconciliation to fix the zero-unowned-gap inventory.
- [ ] `finance` (336): switch the live TS journal flows (`application/opening.ts`, `legacy-revenue.ts`, `journal-review.ts`) onto native `createJournal`/`postJournal`/`reverseJournal` ports so `retire.finance.journals-writers` unblocks; complete goods-journal composition mount into native Commerce fulfillment; then bank-automation and due-draft-jobs packets; family-by-family deletion of the remaining 336.
- Milestone push M-C: KPI core → 0 (global ≈ 978).

### Wave 4 — read tail (queued, starts when an owner's writes hit 0)

`retire.reads.*` per owner, highest-value read families first: finance 1,169 → identity_access 325 → commerce 298 → inventory 285 → files 233 → organization 231 → work_projection 80 → workflow 67 → billing 62 → runtime 47 → customer_authority 46 → records 36 → commercial 31 → manage_erpx 21. Each records a `retirement:ts-statements` pair. Then `audit.read-tail` closes for the sub-300 owners.

## Files / packages to modify

- Rust: `packages/server-{finance,files,identity_access,workflow,billing,organization,work_projection,commerce,inventory,runtime,records,commercial,manage_erpx,customer_authority,shared}/src/{domain,application,adapters,public}/**`
- Mounts (lead-owned): `src/composition/src/**` (new: `inventory_transit_commands.rs`, `inventory_count_commands.rs`, `workflow_projection_worker.rs` projection side-effects, executive vertical, calendar vertical, onboarding slot source)
- Route authority: `packages/feature-<owner>/src/**`, `packages/workplace/src/routes/**` (exact per-route paths named in each task refinement)
- Ledger: `docs/status/rust-delivery-bundles.json` (via coordination CLI only)
- Root `Cargo.toml`/`Cargo.lock` touched only by the lead for new workspace members (none expected — all 15 crates exist)

## Reuse (existing mechanisms — no new infrastructure)

- Native transport pattern: `packages/server-*/src/public/native-*.ts` (thin HTTP clients to the Rust owner) — e.g. `server-identity_access/src/public/native-iam-assignments.ts`, `feature-finance/lib/native-finance-bank*`.
- Route-switch pattern: native-first with `if (native) return native;` fallback in `packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/**` — cutover = promote native to primary and delete the TS branch (live example: `.../finance/[...path]/route.ts`).
- Composition mount pattern: 249 existing mount modules in `src/composition/src/`.
- KPI command: `git grep -I -o -E "INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET" HEAD -- ':(glob)packages/server-*/src/**/*.ts' | wc -l`.
- Coordination: `bun scripts/testing/rust-migration-work.ts` (status/validate read-only for all; mutations by lead).

## Constraints honored

- No staging (3103/hosts/DBs), no 3000/3001/8080, no retired 5433/5435. Dev gateway stays on 3100; private prod 3101 + PG 5432 verified available at end of work.
- Pushes only to `origin/codex/rust-backend-migration`, remote SHA verified, no force.
- Skipped owners' **business** TS stays untouched; only transport-layer consumer repointing where a core owner's TS writer is deleted and an out-of-scope owner imports it (named in the task's `allowedPaths`).
- Acceptance deferred (A1); no full-acceptance, perf, or restore/rollback runs in this wave.

## Verification

- Per slice: `cargo check -p erpx-server-<owner>` + service features, registered catalogue suite for changed behavior, affected frontend typecheck, `retirement:ts-writes` before/after strictly lower; ledger check file recorded.
- Per milestone (M-A/M-B/M-C): combined structure/catalogue/architecture/docs checks, global + core KPI re-measured on pushed SHA, dev gateway `http://127.0.0.1:3100/api/health` 200, affected Holding pages via authenticated browser smoke (user session; report login blockers explicitly), private prod 3101 health 200.
- End of wave: core KPI 0, global ≈ 978, no dual writer, all `retire.*` in-scope tasks closed at `after:0`, read-tail queue queued.
