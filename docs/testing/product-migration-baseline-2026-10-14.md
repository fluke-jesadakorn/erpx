# Product migration baseline and guarded readers — 14 October 2026

Status: **implementation checkpoint; full migration goal active and incomplete**. The [production plan](../production-plan.md#product-separation--2026-10-14) remains the only delivery plan. No product workflow, Company cutover or Commercial enforcement is accepted by this note.

## Baseline and coverage

The initial shared workspace had 965 modified/untracked entries at HEAD `5d21e544ac1621c434b1f58dde941be92f1be45e`. Its status, binary Git diff, HEAD and source-tree digest were retained privately before this implementation. No reset or clean was performed. The pre-change compiled baseline is release `2026-10-14T12-04-35-378Z`, runtime `63fdb5aba201b8430cd8c5e3c085813848b62959dc9659cc62416fd6928b8f1f`. The compiled artifact is retained; comparable performance observations have not been collected.

Read-only snapshots cover the development control database and all three development Holding write databases. They record 15 active Companies (13/1/1 across the Holdings), 516 Holding assignment rows and four Platform assignment rows, including historical rows rather than treating all of them as current authority. Published role revisions, scopes, delegation ceilings, assignment/principal validity, rollout/grant versions, directory metadata and protected-state counts/digests are retained in mode-0600 files under ignored `.scratch/dev/product-migration-baseline-2026-10-14/`. Detailed account/authority rows are not published here. Each database has its own read-only snapshot boundary. All three Holding databases had zero package grants; the snapshot utility has no Apply mode.

The [generated register](../status/product-migration-acceptance.json) now maps every one of the 33 Modules to product/shared placement, package traceability, current owner pages/APIs/services/schema/tests, jobs and registered regression suites. It retains all 187 owner workflows and adds 16 workflows across seven shared/public surfaces. Implementation, verification and activation receipts remain separate and empty. The completion checker rejects their 609 missing current-source phases. Broader performance/security/UI/recovery/compatibility/environment gates remain independent requirements in the production plan.

## Implemented boundaries

- Apps enumerates every authorized Company/Branch/Department, including inherited Departments and directly assigned Departments without elevating their parent Branch. Directory/session/assignment facts are scoped to one render and Company. Applicable-role module ceilings and existing release guards still apply. Grant reads are shared only within that render.
- Office links carry explicit Company context through SSO to the product's `/launch` handler. It accepts only that Office product's destinations, checks current account/directory/session authority and an applicable assignment, then sets a secure host-only Company cookie. Front links encode the actual Company/location. The destination still selects and authorizes its acting role and records. No assignment, grant or rollout is created by selection.
- Commercial exposes the purchased/readiness/authority/record intersection and capability revisions. Command reads require the existing owned Holding transaction, the Company advisory lock used by grant Apply, and share locks on grant rows. A stale serializable snapshot causes the existing whole-transaction retry; a stale caller timestamp cannot authorize an expired package. Time is read on the server after protected reads and owner gates. Transaction isolation and the existing retry policy are unchanged.
- Obligation readers require current Company/package/module-scoped evidence from every required settlement owner, including included handoffs. Missing, stale, wrong-scope or invalid-count evidence rejects inactivation. Concrete owner readers, real provider-authorized administration, legacy grant migration and owner enforcement remain unimplemented.
- The Apps header keeps the account name on its own mobile row. The previous 320px character wrapping was inspected and corrected with shared semantic styling.

These are integration ports and selection behavior. They do not establish live provider authority, package-only business workflows, or UI/API/files/search/Work/jobs/provider enforcement parity. Existing-workspace recovery cards remain necessary and are retained.

## Current candidate and verification

Runtime fingerprint: `b45a9fe15a2d9fbe688350f3f6b602e52e7d1205b8903d7d3dada75737445f27`.

Verification fingerprint: `86d9f459de07dcb3a30683b0754c6c7fa67ff5a289fa9c323e92defe708f32de`.

Isolated release `2026-10-14T12-50-14-168Z` matches that runtime and builds with Bun 1.4.3, Node 26.11.1 and Next 16.4. It has not been deployed. The private verification coordinator checks runtime/assertion fingerprints after every phase and records the raw log digest.

| Check | Result at this checkpoint |
| --- | --- |
| Typecheck | Passed application and tests |
| Lint | Passed, zero errors; 1,091 inherited warnings |
| Architecture / structure | Passed, zero exact legacy edges |
| Catalogue | Passed, 33 Modules / 529 suites |
| Documentation / product registers | Passed; documentation is rechecked after adding this note |
| Complete unit lane | Passed 209 files / 2,824 cases |
| Focused PostgreSQL | Passed three files / 22 cases, including nine package-grant/reader cases and 13 SSO cases |
| Isolated release build | Passed, exact runtime match |
| Complete PostgreSQL lane | Running; no full-lane pass claimed |
| Global UI guard | Failed on inherited frozen/boundary migration work; no acceptance assertion or frozen fingerprint changed |
| Product workflow / performance / restore / cutover | Open |

The earlier complete-lane attempt was deliberately interrupted when review identified the stale-clock/snapshot reader risks. Its 21 passing files and exit 130 remain diagnostic evidence, not a completed or current-candidate result. The corrected expiry and concurrent-draining cases pass without sleeps, threshold increases or assertion removal. The final full lane runs separately on the current frozen source.

All 39 changed/deleted paths in the frozen register have byte-identical diffs to the session's initial workspace snapshot: 36 changed files plus three previously retired entrypoints. That establishes preservation of inherited work, not UI equivalence. The guard still fails; specialized acceptance remains required. No frozen widget/style fingerprint was refreshed.

## Browser and transport observations

The existing authenticated browser session opens Apps and preserves its three legacy recovery links. The current-source recovery view was inspected at actual 320/390/768/1024/1440px widths in **system theme only**, with no horizontal page overflow. Temporary viewport overrides were restored. An interrupted stale-tab viewport attempt is retained separately and is not accepted. Light/dark and all product/package-only surfaces remain open.

Normal SSO plus explicit Company launch reaches Finance Documents for the existing Company and preserves `status=paid`. A missing Company is rejected with `COMPANY_REQUIRED`; the valid Company remains usable afterward. Finance still has the inherited generic shell, so this is context/entry evidence rather than Finance product acceptance. Public Console root navigation reaches the current Apps page through its existing redirect.

All 22 public development health/icon probes passed (44 HTTP 200 observations, successful TLS verification). Anonymous root/alias probes retained four concurrent timeouts on Work/POS/Hotel/Finance, including the repeated concurrent check. Subsequent independent serial checks with the same 25-second limit returned HTTP 200 with valid TLS on all four. Compilation was present in development logs, but no established root-cause or concurrent-stability/performance pass is claimed. Other 18 roots and all three aliases returned HTTP 200. These transport observations do not certify tenant/token/invitation workflows, authenticated cache isolation or product cutover.

## Availability and remaining evidence

Local development `http://127.0.0.1:3100/api/health`, public development `https://console-dev-erpx.jesadakorn.com/api/health` and private production `http://127.0.0.1:3101/api/health` returned HTTP 200. The existing detached pinned development wrapper PID 83242 has parent 1; Next listener PID 83268 serves this workspace on 3100. PostgreSQL remains on shared 5432. The initial development 503 was caused by stale ClamAV definitions; the existing updater refreshed them from its configured source, restoring readiness without relaxing the scanner policy.

No staging access, production deployment, paid certificate, Cloudflare route/configuration change, price/rating change, live collection, Company role assignment or package-grant migration occurred in this checkpoint.

Private evidence is under `.scratch/dev/product-migration-baseline-2026-10-14/`. `verification-current.json` and `verification-coordinator.log` identify the still-running full PostgreSQL lane; `current-*.log` retain its exact-source checks. Preserve the source freeze until it completes, or retain an interrupted diagnostic before making another runtime/assertion change. The main plan owns all remaining implementation and acceptance work.
