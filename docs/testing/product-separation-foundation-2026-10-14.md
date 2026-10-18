# Product separation foundation — 14 October 2026

Status: **scoped Foundation goal complete; full product separation and cutover acceptance remain incomplete**. See the [final scoped candidate](#final-scoped-candidate-after-regression-fixes). This checkpoint preserves the user's full replacement scope in the [active plan](../production-plan.md#product-separation--2026-10-14) and [implementation contract](../contracts/next/product-separation.md). Further owner enforcement, product shells, preferences, public integration, all-host browser/performance and release evidence remain open.

The original `development` worktree had 897 changed/untracked entries. Its diff, status and HEAD were saved under ignored `.scratch/dev/product-split-baseline/` before edits. No original change was reset. A pre-existing browser-test type error was corrected from Playwright `timeoutMs` to `timeout`, retaining the same wait and assertion.

The new registry covers all 33 original modules, 15 products and 22 exact development hosts. Commercial/SSO migrations were applied only to `erpx_dev_control` and `erpx_dev_h001_write`/`h002_write`/`h003_write` on shared `5432`; no legacy grant, role, rollout or billing changes were applied. New flags remain disabled. The migrations add tables and leave existing session/owner tables and identifiers intact.

Verification logs are retained in that ignored directory. Initial checks recorded all-unit 207 files/2,798 tests passing; new store unit/integration tests passed before additional auth-handler regression was added. The final source checks, exact counts and isolated build result are recorded below when complete. Intermediate passes do not certify later edits.

Seven existing desktop browser smoke cases passed on `https://console-dev-erpx.jesadakorn.com`: health/no-store, three signed-out routes, invalid-credential recovery, invalid-reset explanation and password-recovery navigation. The actual login screenshot was inspected: shared brand, form, theme control and actions rendered without visible clipping at 1280×720. This verifies the existing console's signed-out flow only; it is not the required all-product responsive/theme/state matrix or authenticated-product SSO acceptance.

Native browser control returned unavailable surfaces, but repository Playwright smoke tests ran successfully. Cloudflare access remains blocked: direct API reads returned 403, Wrangler reported an expired token whose refresh failed, and provider plugin discovery returned no Cloudflare connector. No credentials were printed and no Cloudflare state or paid subscription was changed. The manifest is desired state; none of the 22 hosts has a DNS/TLS/publication acceptance receipt from this task.

Local/public dev and private production health initially returned HTTP 200. Final availability must be rechecked after all verification and dev must remain detached on `3100`. Staging was not accessed.

## Final source checks

Focused foundation verification passed 15 unit cases and 19 PostgreSQL integration cases. Integration includes real Better Auth credential sign-in, creation-time issuer metadata, secure host-only cookies, copied legacy-cookie denial on reads and raw account mutations, full start/authorize/callback, replay/concurrent consumption, expiry, logout racing a callback, and Company-specific grant authority/idempotency/draining. Test data uses isolated `erpx_test_*` databases.

The complete unit lane passed 207 files / 2,800 cases on the final source, including the environment guard and named Commercial public registry export. Typecheck, lint (zero errors; 1,091 existing warnings), architecture with zero legacy edges, structure, catalogue and manifest checks passed. Focused integration was repeated after the public-export correction and all 19 cases passed. Documentation/generated inventories were reconciled without changing historical acceptance evidence.

The isolated standalone build succeeded at `.runtime/releases/2026-10-14T08-03-55-702Z` with Node 26.11.1, Bun 1.4.3 and Next 16.4. Its runtime source fingerprint is `c7e05192024719d9f28426345f8aab976fb3ea4d085186e94d207fb67f054951`, verified against the final workspace. The verification fingerprint is `3ac0df84ee5df6aaea51ab332148100d9b67f6dbdd1ef1c93319c05d4a5ad9c1`. The build was not deployed. Earlier isolated builds remain historical candidates and are not used to certify this final source.

Existing public-console browser smoke passed seven desktop and seven mobile cases. Both login screenshots were inspected, at 1280×720 and Pixel 7 emulation. Screenshots/traces are preserved under the checkpoint's ignored `browser-desktop/` and `browser-mobile/` directories. No authenticated-product, new-host, specialized-widget or all-theme acceptance is claimed.

The broader integration run completed 55 files with 1,062 passed cases and one failed case before it was interrupted after reproducing that baseline failure. It crossed foundation source revisions and is **not** a final-candidate full regression receipt. The independently reproduced failure is [the IAM destination assertion](../../tests/integration/people_payroll/people-lifecycle.test.ts): expected `technology?view=organization`, received `technology?view=settings&section=groups`. Both that test and its `people-iam-navigation.ts` helper match committed `HEAD` byte-for-byte. Neither was changed to make the gate pass. The test's role destination assertion also still expects the older HR settings route; current helper sends advanced role policy to IT settings.

UI coverage was regenerated and still records 952 pending routes / 1,738 pending sources. `ui:guard` fails because three files in the unchanged [frozen register](../../scripts/ui/frozen-boundaries.json) were already absent at the original baseline:

- `src/app/front/[holdingSlug]/[companySlug]/[app]/[[...path]]/page.tsx`
- `src/app/holdings/[holdingSlug]/hospitality/setup/page.tsx`
- `src/app/holdings/[holdingSlug]/pms/[[...section]]/page.tsx`

Each frozen hash matches its original `HEAD` file; their pre-existing canonical route replacement has not been given specialized browser acceptance here. No fingerprint was refreshed or removed. The unchanged pre-existing `workplace-navigation.ts` diff also contains a trailing-whitespace observation from `git diff --check`.

Full internal browser/provider/restore acceptance, owner commercial enforcement, all-product UI/performance and Cloudflare publication remain open. Intermediate or source-mismatched runs are not reused as final acceptance.

Final availability on 14 October 2026: local dev `http://127.0.0.1:3100/api/health`, public dev `https://console-dev-erpx.jesadakorn.com/api/health`, private production `http://127.0.0.1:3101/api/health`, and the affected existing public `/login` page all returned HTTP 200. The prospective Finance Host returned 421 with routing flags disabled. Dev remains the reused, detached pinned process (wrapper PID 56131, parent PID 1; server PID 56168), with logs at `.scratch/dev/submenu-routes/dev-turbo.log`. Its working directory and listener were rechecked. Shared PostgreSQL remains on 5432. Staging, production deployment and other projects were not changed.

## Scoped Foundation goal

The resumed, explicitly bounded goal closes the source/regression checkpoint only. The full separation and cutover status above remains incomplete. No new flags or hosts are enabled. Historical failures and receipts above retain their original candidate and result.

IAM alias regression was reconciled with the current [People/Technology contract](../contracts/next/hr-it-technology.md): employee aliases resolve to People; role/matrix/review/group policy resolves to Technology Access settings; connections resolve to Technology Directory. Legacy IAM `mappings` means access-group mappings and remains in Technology Access settings, distinct from Directory field mappings. Runtime navigation behavior is unchanged. Exact assertions cover all these destinations and escaping, replacing the obsolete People-settings/organization expectations rather than dropping coverage.

The three retired frozen entrypoints have an explicit [successor register](../../scripts/ui/frozen-retirements.json). The original [frozen hashes](../../scripts/ui/frozen-boundaries.json) are unchanged and verified against immutable commit `5d21e544ac1621c434b1f58dde941be92f1be45e`. Review found extraction into Front daily screens/metadata and Holding presenters, canonical link/detail dispatch changes, and retained owner/context/read-recovery boundaries. The guard checks the old source is absent and the listed successor bytes remain fixed. It rejects missing review/evidence, altered successors and resurrection of the old catch-all. It does **not** assert behavioral equivalence or specialized acceptance: the new register explicitly stays pending and the completion gate still fails. Fourteen guard regression cases run the real checker in isolated repositories, including these failure modes. The inherited whitespace observation is cleaned without changing navigation behavior.

Final candidate checks and availability are recorded below after the complete run. Logs for this goal are retained under ignored `.scratch/dev/product-foundation-goal/`; earlier interrupted runs remain in `.scratch/dev/product-split-baseline/`.

### Superseded pre-HR candidate

Runtime fingerprint: `f4cbf26e5a3d7226ebbcb1dff6a0d6e3a67bc0b94faa8beaa7de821ec3d62529`. Verification fingerprint: `d9ad089e98de771ec61ada5bf81e85753581d2e16dbe45ea69f505caeb91e986`. Isolated release: `.runtime/releases/2026-10-14T08-27-39-009Z`, built successfully with pinned Node 26.11.1/Bun 1.4.3/Next 16.4; no deployment. The earlier `08-21-33` build and interrupted pre-freeze/alias-review integration runs remain diagnostic records, not this candidate's acceptance.

| Check | Scoped result |
| --- | --- |
| Typecheck | Passed application and test projects |
| Lint | Passed, zero errors; 1,091 inherited warnings retained |
| Architecture / structure | Passed; zero legacy edges |
| Catalogue / product manifest | Passed; 33 modules, 527 suites; 15 products / 22 desired hosts |
| Documentation | Passed; final documentation is rechecked after recording results |
| Complete unit lane | Passed 207 files / 2,807 cases, including 14 actual-checker UI guard regressions |
| Complete PostgreSQL integration lane | Diagnostic only: six failures retained; later runtime changes require a new complete run |
| Public-console browser | Passed all 8 smoke cases on desktop and all 8 on mobile, using normal authorized sign-in; session persists after reload |
| Ready-page visual inspection | Today content inspected after loading on desktop and Pixel 7, no page overflow; canonical Administration/Technology Directory connections page opens authenticated on both |
| Frozen route retirements | Original evidence and all successor hashes verified; no retirement evidence errors |
| Global UI source guard | Still fails: 36 inherited frozen differences and 467 inherited boundary consumers; no assertions/fingerprints suppressed |
| Global UI completion | Still blocked: 3 retired boundaries, 1,738 pending sources and 952 pending routes; no all-product UI acceptance claimed |
| Whitespace | `git diff --check` passed |

The 36 changed frozen-file diffs were compared byte-for-byte with the saved pre-existing patch; all 36 match. The guard's old missing-file crash had previously masked these further differences. Full UI migration/acceptance is a separate open delivery gate: this checkpoint exposes those failures and reconciles only the three retired entrypoint references. It does not authorize updating any frozen widget/style fingerprint. The boundary-consumer report remains available in full in `ui-source-final.log`.

Additional browser diagnostics probed removed `/iam/mappings` and `/iam/connections` paths and received 404. Those probes do not certify legacy redirect compatibility. Current canonical destinations were then checked directly; full alias/cutover acceptance remains open in the parent plan. Ready screenshots are `ready-desktop.png` and `ready-mobile.png`; final smoke artifacts are under `browser-desktop-final/` and `browser-mobile-final/` in the ignored goal directory.

The first broad candidate also exposed an inherited upgrade-fixture error: `command-receipts-upgrade.test.ts` compared every work-cache revision before/after migration 1040 while invoking all later migrations too. Later IAM migrations legitimately modify roles/permissions and register new tables. The test now copies 1040 into its isolated migration root and replays that root. All original exact receipt/FK/ACL/trigger/revision assertions remain intact; no production migration changed. Both upgrade cases passed focused verification. The broad pre-fix run and its failure remain in `integration-before-upgrade-fixture-fix.log`; a new final-source full lane is required and running. This is a test-only correction: runtime fingerprint and compiled artifact remain unchanged; verification fingerprint is updated above.

The subsequent diagnostic lane found two further migration-fixture instances (1038 PMS cache and 1039 transaction-local assertions). Both now isolate and replay their named target migration, retaining all exact original cache/receipt assertions. Guided intake now asserts the current canonical purchase URL exactly instead of the retired request path. Member-directory regression now asserts that unlinked accounts are absent from the employee-only HR directory; reviewed identity linking and the existing beta display fallback remain explicitly checked. These corrections preserve the active contracts rather than weakening checks.

The Directory failure was a real inherited command regression after HR lost `iam.read`: person-field commands still requested the entire IAM management snapshot. A private command projection now reads only the scope-checked Person, Company person-field definitions and target value row for writers lacking IAM read. It retains field ownership/ACL/Branch restrictions, current command authority, preview hash/version guards and existing reconciliation. Full IAM reads still deny HR. Two added unit cases verify writing without reading groups/policies/candidates or gaining IAM permission, and rejecting changed field ACLs between preview/apply. The unchanged 22-case Directory integration suite and 9-case Guided intake suite passed together; the two isolated migration tests and four member-directory cases also passed. This runtime correction requires a new compiled candidate and complete final-source checks below; earlier artifacts do not certify it.

### Final scoped candidate after regression fixes

Runtime fingerprint: `9afb82520fe3409522d78a0fb89621e9548b477e32e6ff2a1748089297ba6937`. Verification fingerprint: `450449c0582a9ce742e8aa0afcde806f663b656f9804d2369bb71706b5aeada7`. The full diagnostic lane finished with 223 files passing / 6 failing and 2,129 cases passing / 6 failing. It crossed the HR fix and is retained strictly as diagnostic evidence in `integration-six-failures-diagnostic.log`. All six failures were reproduced and then passed focused regression after the corrections described above. The Billing case now checks the exact canonical `/administration/usage` return URL; no checkout, price, billing or provider behavior changed.

The complete final-source PostgreSQL lane passed **229 files / 2,135 cases**, exit 0, in 1,828.90 seconds. The complete unit lane passed **207 files / 2,809 cases**. Typecheck, lint (zero errors / 1,091 inherited warnings), architecture (zero legacy edges), structure, catalogue, documentation and product manifest checks passed. All 8 public-console smoke cases passed on desktop and all 8 on mobile, including normal authorized credential sign-in and retained session after reload. Final ready Today screenshots and the canonical Directory connections page were checked separately through the same public origin.

The isolated release `.runtime/releases/2026-10-14T09-16-23-380Z` built successfully with pinned Node 26.11.1, Bun 1.4.3 and Next 16.4. Its runtime hash matches the final source above; the final verification hash also matches the frozen source. This release has not been deployed. `git diff --check` passed.

The scoped Foundation **implementation/verification checkpoint is complete**. Global UI source acceptance remains failed on the inherited 36 frozen differences and 467 boundary consumers; the three retired entrypoints have valid original evidence and unchanged reviewed successors. The completion gate still rejects 3 retired boundaries, 1,738 pending sources and 952 pending routes. Neither old fingerprints nor acceptance assertions were refreshed to make those gates pass. These remain parent-plan migration/acceptance work, alongside real administrator activation, commercial enforcement, all-product shells/preferences, public integrations, 22-host publication and compiled performance/restore acceptance. Routing and SSO flags remain disabled.

Final availability: local dev `http://127.0.0.1:3100/api/health`, public dev `https://console-dev-erpx.jesadakorn.com/api/health`, private production `http://127.0.0.1:3101/api/health`, and public `/login` returned **HTTP 200**. PostgreSQL on shared 5432 reports 18.6 and remains available. The existing detached pinned dev wrapper PID 56131 (parent 1), Next server PID 56168, listener 3100 and workspace working directory were rechecked. Logs remain at `.scratch/dev/submenu-routes/dev-turbo.log`. No staging, production deployment, paid certificate, DNS/Tunnel or other-project service changes were made. The disabled prospective Finance host still returns 421.

Final logs/receipts are under ignored `.scratch/dev/product-foundation-goal/`: `candidate-final.json`, `checks-final.json`, `integration.log`, `build-hr.log`, `browser-final.json`, browser artifacts/screenshots, `ui-hr-final.log` and the full inherited-frozen-diff audit. Earlier interrupted, failing or superseded runs remain diagnostic evidence and do not certify this candidate.
