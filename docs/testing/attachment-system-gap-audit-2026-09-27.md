# Attachment platform closeout and release-readiness report — 2026-09-27

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

> Historical snapshot. The 133 development reconciliation blockers described below were resolved with explicit operator approval on 2026-09-28. See [the reconciliation closeout](attachment-reconciliation-closeout-2026-09-28.md) for the final zero-blocker receipt, backup, and remaining release gates. The rest of this report records the pre-remediation state.

## Decision

The universal attachment implementation in the current working tree is materially complete at the platform, policy, domain-integration, compatibility, and shared-UI layers. The earlier audit's tenant-boundary, legacy-bypass, Billing-contract, immutability, backfill, publication, and sensitive-document findings are no longer accurate.

The global attachment gate is **not approved to open yet**. The frozen reconciliation correctly produced `gateReady: false`: legacy development data still contains unknown-purpose fixtures, unavailable malformed files, missing ledger entries for the unknown sources, quota deltas, and Billing orphan objects. A subsequent no-apply audit, run with the same healthy scanner and preview environment as the service, now turns those findings into a deterministic, SHA-256-bound, read-only adjudication manifest with **133 deduplicated work items**. It does not approve, mutate, relabel, or remove any record.

Repository and object provenance for the current blocker population has been traced to synthetic local seed, acceptance, and E2E fixtures; no blocker was identified as user evidence. The forward fixture generators have been corrected so they do not recreate the ambiguous Leave purpose, CSV task evidence outside policy, malformed PDF bodies, or invalid PNG bytes. The already-persisted records remain intact. Retiring or resetting those synthetic records still requires an explicit approved operator decision; provenance alone is not authorization to change the databases.

Local migrations and a frozen final reconciliation have now run. This report does not claim that the gate is open, that browser acceptance is complete, or that staging/private production have been upgraded; both installed environments still run an older release.

## Current environment evidence

| Area | Verified result | Release meaning |
| --- | --- | --- |
| Development attachment runtime | ClamAV 1.5.4 is ready with current definitions; preview and jobs readiness checks pass | The scanner/preview/jobs prerequisite is available locally. The global gate remains false. |
| Development service after closeout | Local and public `/api/health` return HTTP 200; the persistent web/jobs processes report scanner, preview, jobs, and migrations ready, `uploadFrozen: false`, and `enabled: false` | Development was restored from the temporary reconciliation freeze without opening the global gate. |
| Local schema | Holding tracks are applied through `0340_attachment_jobs_storage_grant.sql`; control is applied through `0021_billing_attachment_reconciliation.sql` | `bun run db:migrate:local` could not create databases because the configured admin role lacks that permission. The existing databases were migrated with the repository's migration runner and migration roles instead; this was a provisioning-permission limitation, not a migration bypass. |
| Frozen final reconciliation | Completed with scanning and checksums; `gateReady: false` | Cross-Company, missing-object, checksum, and orphan checks are clean for Holdings, but legacy ownership/state/ledger/quota issues remain. Billing has 23 orphan objects. Details are recorded below. |
| Read-only adjudication audit | 133 deduplicated work items with a source-report digest, per-item blockers, required evidence, allowed resolutions, prohibited shortcuts, and blank decision fields | This is an operator worklist, not a migration or approval. Overlapping source findings are coalesced and the live records remain unchanged. |
| Build artifact | Isolated deploy build succeeded; current release is `.runtime/releases/2026-09-27T15-55-11-076Z` | The deploy artifact is buildable without sharing the dev `.next` directory. |
| Staging/private production | Still on an older installed release | Their health does not establish readiness of this attachment release, and neither environment has been migrated or activated by this audit. |
| Required health endpoints | Local/public development, local/public staging, and private production all return HTTP 200 | Staging and production responses do not contain the new attachment readiness contract because they remain on the older release. |

## Resolved findings from the earlier audit

| Earlier finding | Current evidence and resolution |
| --- | --- |
| Attachment-set mutation was not tenant-scoped | All request-facing set/link loaders in the central service include `company_id`; reserve, reuse, status, content, withdrawal, publication, and binding use the actor's Company. Cross-Company guessed identifiers are covered in the canonical lifecycle tests. See [attachment service](../../src/server/modules/files/public/attachments/service.ts) and [canonical lifecycle tests](../../tests/integration/shared.attachments/attachments-v2.test.ts). |
| Legacy routes bypassed quarantine and scanning | The one-release `/uploads` adapter creates an approved canonical set/link even while the UI gate is false, writes to quarantine, scans immediately for compatibility callers, and either binds or stages for the host domain. The source-ID read route resolves an authorized link and has no raw-source fallback. See [legacy reserve adapter](../../src/app/api/v1/holdings/%5BholdingSlug%5D/uploads/route.ts), [legacy content adapter](../../src/app/api/v1/holdings/%5BholdingSlug%5D/uploads/%5Bid%5D/content/route.ts), and [source-ID read adapter](../../src/app/api/v1/holdings/%5BholdingSlug%5D/attachments/%5Bid%5D/content/route.ts). |
| Billing migration/runtime were incompatible | Control migrations preserve the historical object key, add immutable lifecycle/audit/reconciliation records, and queue historical files without declaring them clean. Runtime intake uses explicit columns, a 10 MiB limit, quarantine, shared ClamAV scanning, state-gated reads, access events, and a separate storage ledger. See [0020 lifecycle](../../migrations/postgres/control/0020_billing_attachment_lifecycle.sql), [0021 reconciliation](../../migrations/postgres/control/0021_billing_attachment_reconciliation.sql), [Billing API](../../src/server/modules/billing/public/billing-api.ts), and [Billing scanner/reconciler](../../src/server/modules/billing/public/billing-attachments.ts). |
| The gate had no data-readiness proof | The reconciler backfills legacy and specialized owners, queues real historical bytes for scanning, verifies objects/checksums/ledger/quota/orphans, writes per-Holding and Billing final receipts, and emits `gateReady`. Final mode refuses to run without a disabled gate, upload freeze, apply, scanning, checksum verification, and healthy runtime prerequisites. Read-only runs additionally produce a digest-bound adjudication manifest that merges blockers for the same source without guessing a new purpose. See [reconciler](../../scripts/local/reconcile-attachments.ts), [adjudication manifest](../../src/server/modules/files/public/attachments/reconciliation-adjudication.ts), [migration helpers](../../src/server/modules/files/public/attachments/migration.ts), [health route](../../src/app/api/health/route.ts), and [rollout runbook](../../ops/macos/attachment-rollout.md). |
| Source/set immutability and corrections were incomplete | `0331` freezes physical source identity after scan completion, freezes bound sets and links, protects append-only audit tables, and constrains publication/storage mutation. The reserve API supports a same-record/purpose `supersedesLinkId`, and tests exercise correction linkage and bound-link immutability. See [0331 schema](../../migrations/postgres/holding-write/0331_attachment_sources_links.sql) and [canonical lifecycle tests](../../tests/integration/shared.attachments/attachments-v2.test.ts). |
| Cross-record reuse could expand audience silently | Reuse requires current source-read and destination-write authorization, same Company, classification dominance, exact source/destination audience snapshots, and an expansion event. Public or portal-visible sources cannot move to another owner. Publication must stay on the authorized source owner. Link/status/content/publication joins require source/set Company agreement, while reconciliation reports malformed link, intent, storage, and publication ownership as gate blockers. See [reuse/publication service](../../src/server/modules/files/public/attachments/service.ts) and [reconciliation](../../src/server/modules/files/public/attachments/migration.ts). |
| CRM binding authority could change after preparation | The atomic target guard now receives the actor and rechecks Opportunity lead owner, active Sales eligibility, Branch, and Department without relying on the unchanged Opportunity version. CRM Account binding also rechecks the scoped lead relationship. Five focused regressions mutate owner, eligibility, Branch, Department, and Account scope between preparation and guard execution. See [binding guard](../../src/server/modules/files/public/attachments/service.ts) and [CRM atomic regressions](../../tests/integration/shared.attachments/attachment-crm-atomic-guard.test.ts). |
| Leave evidence could race final approval or cleanup | Leave attachment mutation, completion, withdrawal, reuse, binding, and expiry serialize on the owner row; the lock path rechecks session/assignment and final request state. Focused tests cover approval races, bound-set protection, idempotent completion, cleanup serialization, abandoned expiry, and revocation. See [attachment service](../../src/server/modules/files/public/attachments/service.ts), [Leave service](../../src/server/modules/people_payroll/public/leave.ts), and [Leave lock regressions](../../tests/integration/people_payroll/leave-attachment-lock.test.ts). |
| Sensitive People/Payroll/Talent/Learning/Benefits documents used broad record permissions | Dedicated document permissions and closed typed-purpose adapters now enforce subject ownership, workflow assignment, self-visible allowlists, and metadata denial for hidden HR purposes. Retirement-clearance evidence is task-scoped. See [permission migration](../../migrations/postgres/holding-write/0333_sensitive_document_permissions.sql), [sensitive policy](../../src/server/modules/files/public/attachments/sensitive-policy.ts), and [sensitive access tests](../../tests/integration/shared.attachments/sensitive-attachment-access.test.ts). |
| Support internal/customer-shareable evidence and portals were not separated | Support purposes have different staff rules; customer access is an authenticated explicit-owner route that returns only customer-shareable sanitized previews. Legal portal uploads also enter the canonical scan lifecycle. See [portal/collaboration policy](../../src/server/modules/files/public/attachments/portal-collaboration-policy.ts), [customer portal adapter](../../src/server/modules/customer_360/public/customer-portal-attachments.ts), [customer route](../../src/app/api/customer-portal/%5BholdingSlug%5D/attachments/%5B%5B...path%5D%5D/route.ts), and [Legal portal adapter](../../src/server/modules/legal/public/legal/portal-attachments.ts). |
| Record history disappeared after owner version changes | The shared panel lists bound history across owner revisions, keeps only the current editable draft active, renders a read-only manifest, supplies sanitized thumbnails, retries interrupted uploads, and blocks form submission while selected files are unscanned. List/console records can use the shared drawer. See [AttachmentPanel](../../src/components/files/attachment-panel.tsx), [record panel](../../src/components/files/record-document-panel.tsx), and [record drawer](../../src/components/files/record-document-drawer.tsx). |
| Closed-gate and rollback UI exposed unusable attachment controls | The application root now provides one server-derived attachment feature mode to Holding, Front, and portal surfaces; the context itself defaults to disabled if a consumer is ever rendered outside that boundary. A disabled gate hides panels, drawers, custom document triggers, and empty attachment-only cards; rollback read-only mode preserves readable evidence while forcing mutation controls off. Staged set IDs and hidden form fields are suppressed immediately when mutation authority closes, while Support can still navigate both readable evidence purposes. Legacy Brand upload/remove and Support bind/withdraw entry points also reject rollback writes server-side, and Brand publication review controls use the same review permissions as the service. Withdrawal now returns the canonical post-withdraw status shape, the label prioritizes terminal link state over an available physical source, and publication reorder/revoke controls require current authority and state. See [root layout](../../src/app/layout.tsx), [feature context](../../src/components/files/attachment-feature-context.tsx), [AttachmentPanel](../../src/components/files/attachment-panel.tsx), [withdraw route](../../src/app/api/v1/holdings/%5BholdingSlug%5D/attachment-links/%5BlinkId%5D/withdraw/route.ts), [state helpers](../../src/lib/files/attachment-state.ts), and [publication panel](../../src/components/files/media-publication-panel.tsx). |
| Messaging, Expense, Legal, and Billing were lifecycle exceptions | Messaging CSV, Expense CSV, Legal DOCX, authenticated Legal portal files, and Billing evidence retain their format exceptions but now use quarantine/scan/state/audit behavior. The specialized suites cover those boundaries. See [specialized adapter tests](../../tests/integration/shared.attachments/specialized-attachment-adapters.test.ts) and [Billing tests](../../tests/integration/commercial/billing-api.test.ts). |
| Public storefront media could fall back to a private original | Public brand/storefront reads now require an approved publication derivative; no publication produces a 404 and never reads the original source object. See [front app source](../../src/server/modules/work_projection/public/front-app.ts) and [front logo regression](../../tests/integration/brand/front-logo.test.ts). |
| Expense AI could start file-content processing implicitly | The UI performs only a status read on mount. File preparation and AI queueing require the user's explicit “AI extract” action and the POST contract requires `contentAccessConsent: true`. Canonical/read-only lifecycle filters apply to the evidence manifest. See [Expense AI panel](../../src/components/people_payroll/expense-ai-panel.tsx), [Expense API](../../src/app/api/v1/holdings/%5BholdingSlug%5D/expenses/%5B%5B...path%5D%5D/route.ts), and [AI provider tests](../../tests/integration/agent/ai-providers.test.ts). |

## Implemented architecture

### Canonical ownership and schema

- [ADR-036](../adr/ADR-036-canonical-file-sources-and-policy-bound-links.md), the [attachment contract](../contracts/next/attachments.md), and the glossary define File Source, File Set, File Link, Bound File, and Media Publication without redefining Evidence or Policy Attachment.
- Holding migrations `0331` through `0340` add source lifecycle metadata, set/link ownership, scan attempts, append-only events/access, publication order/revision, storage accounting, derivative accounting, security-destruction sealing, Legal Hold race guards, and the jobs-role storage grant required by reconciliation. The control database uses `0020` and `0021` for Billing.
- The current closed registry contains **91 target types, 201 target/purpose policies, and 216 exact target/purpose/component tuples**. [The exact-set test](../../tests/integration/shared.attachments/attachments-v2.test.ts) pins the tuple SHA-256 to `bf3ed07afb05911fb8a57061075a914f757feb0af1ab1f2fd124b97f017135b9`, rejects duplicates/unknown tuples, and enforces the 1–100 file-policy range.
- No policy target directly owns stock balance, ledger, report, Today, Calendar, or Executive projections. Those surfaces retain source-record links.

### Lifecycle and security boundary

- Reservations last 15 minutes and do not change the owner version. Upload reads are bounded streams; checksum and content-derived MIME are verified before ClamAV. Only `available` sources can bind or read. See [feature constants](../../src/server/modules/files/public/attachments/feature.ts), [bounded upload](../../src/server/modules/files/public/attachments/upload.ts), [scanner](../../src/server/modules/files/public/attachments/scanner.ts), and [service](../../src/server/modules/files/public/attachments/service.ts).
- Scanner readiness requires a Unix socket and definitions no older than 24 hours. The native manifest pins ClamAV 1.5.4, libheif 1.23.5, and Poppler 26.09.0; the verifier rejects TCP configuration and checks launchd, socket group/mode, versions, and definition age. See [runtime manifest](../../ops/macos/clamav/runtime-versions.json), [installer](../../ops/macos/install-attachment-runtime.py), and [runtime verifier](../../ops/macos/clamav/verify-runtime.sh).
- HEIC/HEIF and raster previews are decoded into bounded metadata-stripped output; PDF previews use bounded Poppler. Originals are downloads; only generated previews are inline with private/no-store, `nosniff`, and sandbox CSP. See [preview service](../../src/server/modules/files/public/attachments/preview.ts) and [link content route](../../src/app/api/v1/holdings/%5BholdingSlug%5D/attachment-links/%5BlinkId%5D/content/route.ts).
- Clean drafts expire at the exact seven-day boundary. Cleanup freezes with upload intake, respects active links and the longest retention snapshot, checks both canonical Legal ownership and preservation records, serializes against Legal Hold, and deletes every ledger-backed physical variant only after sealing database state. Malware and terminal scan errors remain sealed. See [cleanup/destruction service](../../src/server/modules/files/public/attachments/service.ts), [security seal](../../migrations/postgres/holding-write/0336_attachment_security_destruction_seal.sql), and [Legal Hold guard](../../migrations/postgres/holding-write/0339_legal_attachment_hold_race_guard.sql).
- Emergency rollback is explicit: gate false, read-only true, freeze true. Only clean bound evidence remains readable; reserve, upload, link, bind, withdrawal, cleanup, and publication mutations remain closed.

### API and shared UI

The implemented authenticated API includes set list/create, file reserve, upload completion, status, original/preview content, withdrawal, same-Company reuse, record bind, sanitization attestation, publication create/list/approve/revoke/reorder, compatibility source reads, authenticated customer upload/read, and approved public derivative delivery. The route inventory is under [`src/app/api`](../../src/app/api).

The shared `AttachmentPanel` supports multi-select, drag/drop, mobile camera input, captions, scan progress, interrupted-upload retry, sanitized thumbnails/PDF preview links, original download, reasoned withdrawal, immutable state, history, read-only mode, and selected-file submission blocking. `RecordDocumentPanel` and `RecordDocumentDrawer` reuse it for detail and console/list surfaces. The application-root feature context hides these surfaces and custom triggers while the gate is disabled, and forces them read-only during rollback. Withdrawal consumes the same canonical status contract as polling, and publication ordering is available only to create/review actors. The current tree has 57 TSX consumers of these shared attachment components.

### Domain coverage

The current registry covers the real owners enumerated in the approved plan: Procurement/AP; Inventory/Product/Quality; Sales/CRM/Commerce/Marketing; Finance/POS/PMS; Leave/Workforce; People/Payroll/Talent/Learning/Benefits; Assets/Maintenance/Field Service; Manufacturing/Planning; Projects/Collaboration/Support; Legal/Organization/IAM/Platform; and authenticated portals/public derivatives.

There are 28 non-platform server modules that call `prepareAttachmentBinding`, so action evidence is bound with the host command rather than by upload completion. Examples include partial purchase receipt/POD/return in [Commerce](../../src/server/modules/commerce/public/commerce.ts), counts and transit in [Inventory counts](../../src/server/modules/inventory/public/inventory-counts.ts) and [Inventory transit](../../src/server/modules/inventory/public/inventory-transit.ts), inspection/disposition in [Quality](../../src/server/modules/quality/public/quality.ts), Messaging and Legal completion, Finance/POS/PMS, overtime/time correction, supplier payment/refund, Support incident/evidence, payroll correction, and Organization review.

The complete owner/purpose summary is maintained in the [attachment contract](../contracts/next/attachments.md); API-only record owners intentionally did not gain placeholder pages.

### Leave-specific behavior

- `0332_leave_evidence_policy.sql` versions the threshold, exact purpose/count, due stage, grace period, privacy, effective date, and explanation.
- A request may be submitted before its certificate. Final HR approval requires the current clean `medical_certificate` count; a supporting photo does not satisfy that purpose.
- Each decision pins `{linkId, sourceId, purposeKey, checksumSha256, mediaType, bytes}` and binds the reviewed draft links in the same Leave transaction. Distinct physical sources, not duplicate links, satisfy the count.
- Medical evidence is visible only to the requester, current eligible approver, a decision-recorded approver, or `leave.evidence.read`; removing an undecided approver revokes access immediately.
- Leave owner-row locking serializes evidence mutation/expiry with approval manifest creation. See [Leave service](../../src/server/modules/people_payroll/public/leave.ts), [Leave evidence policy](../../src/server/modules/people_payroll/public/leave-evidence.ts), and [self-service journey tests](../../tests/integration/people_payroll/self-service-journeys.test.ts).

## Test assets present in the repository

The source tree now has 58 TypeScript test files with attachment references. The attachment-focused suites provide the following evidence:

| Suite | Coverage present |
| --- | --- |
| [attachments-v2.test.ts](../../tests/integration/shared.attachments/attachments-v2.test.ts) | Exact registry, unknown legacy quarantine, tenant isolation, concurrent reserve/quota/count, 10 MiB boundary, scan/bind/read/withdraw/supersede, seven-day boundary, publication/revocation, immutable schema, cleanup/Legal Hold, specialized backfill, derivative/generated-artifact reconciliation. |
| [attachment-formats.test.ts](../../tests/integration/shared.attachments/attachment-formats.test.ts) | Live ClamAV/EICAR, unavailable/stale scanner, real rotated HEIC, metadata stripping, stream bound, timeout, malformed/polyglot rejection, decompression-bomb dimensions. |
| [attachment-ops-health.test.ts](../../tests/unit/shared.attachments/attachment-ops-health.test.ts) | Gate-off compatibility readiness, jobs/freeze agreement, final Holding/Billing receipts, pinned migration order and local-only scanner manifest. |
| [attachment-gate-contract.test.ts](../../tests/unit/shared.attachments/attachment-gate-contract.test.ts) | Exact global-gate, read-only rollback and upload-freeze values, fail-closed reads/writes, and pinned lifecycle limits. |
| [attachment-feature-context.test.tsx](../../tests/unit/shared.attachments/attachment-feature-context.test.tsx), [attachment-state.test.ts](../../tests/unit/shared.attachments/attachment-state.test.ts), [attachment-withdraw-route.test.ts](../../tests/unit/shared.attachments/attachment-withdraw-route.test.ts) | Application-root gate propagation, fail-closed custom surfaces, enabled-only staged-set propagation, rollback hidden-field suppression, terminal link-state labels, publication reorder/revoke visibility, and the canonical post-withdraw response contract. |
| [attachment-reconciliation-adjudication.test.ts](../../tests/unit/shared.attachments/attachment-reconciliation-adjudication.test.ts) | Deterministic digest-bound operator worklist, merged per-source blockers, complete-inventory detection, prerequisite failures, and non-truncated streamed output. |
| [specialized-attachment-adapters.test.ts](../../tests/integration/shared.attachments/specialized-attachment-adapters.test.ts) | Messaging CSV, Expense CSV, Legal staff/portal DOCX quarantine, clean binding, guessed-link denial, closed specialized formats. |
| [sensitive-attachment-access.test.ts](../../tests/integration/shared.attachments/sensitive-attachment-access.test.ts) | Exact sensitive inventory, self-visible metadata boundary, employee certificate bind/version, Talent assignment, retirement-clearance task isolation. |
| [finance-operations-attachments.test.ts](../../tests/integration/finance/finance-operations-attachments.test.ts) | Exact Finance/POS/PMS/Organization/IAM inventory, forbidden secret-like artifacts, Company/Branch/permission denial, atomic/stale binding, sanitization attestation. |
| [self-service-journeys.test.ts](../../tests/integration/people_payroll/self-service-journeys.test.ts) | Leave submit-before-certificate, wrong-purpose denial, final approval block, immutable manifests, gate-off compatibility, cross-Company denial, removed approver denial, distinct-source count. |
| [leave-attachment-lock.test.ts](../../tests/integration/people_payroll/leave-attachment-lock.test.ts) | Seven Leave concurrency regressions covering final-state mutation denial, approval serialization, bound completion replay, cleanup/expiry ordering, and session/assignment revocation. |
| [attachment-crm-atomic-guard.test.ts](../../tests/integration/shared.attachments/attachment-crm-atomic-guard.test.ts) | Five atomic CRM regressions covering owner reassignment, eligibility suspension, Branch drift, Department drift, and CRM Account scope drift. |
| [front-logo.test.ts](../../tests/integration/brand/front-logo.test.ts) | No public fallback to a legacy/private storefront original. |
| [attachments-v2.spec.ts](../../tests/e2e/shared.attachments/attachments-v2.spec.ts) | Desktop/mobile Leave file picker and camera contract, multi-upload, interrupted retry, scan status, sanitized preview headers, original download headers, withdrawal, reload persistence. |

The following is the completed post-correction verification snapshot:

- Typecheck passes after the final rollback/UI changes.
- Full lint completes with **0 errors and 1 unrelated warning** in `src/components/field_service/service-navigation.tsx`.
- The final focused attachment/rollback reruns pass **8 files / 49 tests**, followed by **2 files / 17 tests** after the last staged-state suppression adjustment.
- The final post-patch integration run passes **179 files** with 2 skipped and **1,376 tests** with 3 skipped in 438.09 seconds.
- The canonical browser suite exits successfully in both desktop and mobile projects, with its one test intentionally skipped in each project because the global attachment gate is closed. It was not bypassed or temporarily opened for acceptance.
- The isolated deploy build succeeds and produced `.runtime/releases/2026-09-27T15-55-11-076Z`.
- A fresh no-apply checksum audit with the configured healthy scanner/preview runtime still produces exactly **133 blocking adjudication items** and `gateReady: false`; no persisted evidence was mutated.

## Migration and frozen reconciliation evidence

Local Holding migrations are applied through `0340_attachment_jobs_storage_grant.sql`, and control migrations are applied through `0021_billing_attachment_reconciliation.sql`. The normal `db:migrate:local` command stopped at its create-database step because the configured admin role does not have database-creation permission. The audit therefore used the existing repository migration runner against the already-provisioned databases with the intended migration roles; migration checksums and ordering remained enforced.

The frozen final reconciliation ran with `--apply --scan --checksum --final`. It intentionally retained the closed global gate and reported:

| Database | Unknown ownership/purpose | Unavailable | Missing ledger | Quota delta | Cross-Company defects | Missing/checksum/orphan defects |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| H001 | 21 | 92 | 21 | -6,977 | 0 | 0 |
| H002 | 15 | 15 | 15 | -6,808 | 0 | 0 |
| H003 | 0 | 0 | 0 | 0 | 0 | 0 |
| Billing | — | — | — | 0 | — | 23 orphan objects; no missing-object or checksum defect |

A later no-apply reconciliation audit generated a **133-item read-only adjudication manifest** while using the configured healthy ClamAV and preview runtime. Each item is tied to the source report digest, lists its blocker or merged blockers, states the evidence needed for a decision, enumerates only policy-safe resolution paths, and starts with an empty pending decision. Its category counts intentionally overlap: for example, unknown Company scope and missing ledger findings on an unavailable source are merged into the same source work item rather than duplicated. The 133 items comprise the deduplicated data/storage decisions plus the required future frozen-final-reconciliation step; the manifest neither replaces the frozen receipt nor changes the database.

The 107 unavailable Holding records break down as follows:

- 36 policy/Company-unknown sources: 30 seeded `leave evidence` PDF/CSV files and 6 `staff_task` CSV/text files outside the locked policy inventory. The same 36 sources have missing-ledger findings, which the manifest merges into their source items.
- 71 H001 scan-error sources with exact current groups: 25 `invoice_payment` / `bank_payment_slip` PDFs, 22 `commerce_order_delivery` / `proof_of_delivery` PDFs, and 24 `social_draft` / image PNGs.

The two Holding quota deltas and 23 Billing orphan objects remain separate decisions. Repository history, deterministic identifiers, object metadata, and generator call sites trace the entire current blocker population to synthetic local seed, acceptance, and E2E fixtures. The forward generators have been corrected: the local seed no longer inserts ambiguous legacy Leave evidence; task evidence now uses a policy-supported image; business and inventory verification use a valid PDF fixture; and social E2E uses a decodable PNG fixture. These changes prevent recurrence only. No existing source, storage object, ledger row, quota row, or audit record was deleted, rewritten, relabelled, or declared clean.

The preserved live synthetic records still require an explicit approved retirement/reset or another policy-supported adjudication outcome. They must not be silently relabelled, declared clean, or deleted to force readiness. The quota and ledger discrepancies remain release blockers until that decision is executed and reconciled; the 23 Billing orphan objects likewise require an explicit owner decision.

During the frozen run, the audit found and fixed two runtime defects: scanner definition modification time needed bigint-safe handling, and the jobs role needed the storage-ledger `INSERT` grant supplied by `0340`. Four files affected by those defects were retried and scanned clean. ClamAV 1.5.4, definitions, preview, and jobs readiness now pass locally.

The temporary upload freeze used for reconciliation has been removed without opening the feature gate. `ERPX_ATTACHMENTS_V2` remains false and must stay false until the preserved synthetic records receive explicit approved disposition and a repeated frozen final reconciliation produces a clean receipt.

## Remaining acceptance sequence

After an operator explicitly approves and executes the disposition of the preserved synthetic fixtures, ledger and quota deltas, and Billing orphans, repeat the frozen final reconciliation. Generator provenance is supporting evidence, not an automatic deletion/reset instruction. Opening the gate requires `gateReady: true`, ready receipts for every Holding and Billing database, and zero unresolved ownership, availability, Cross-Company, object, checksum, ledger, orphan, quota, or duplicate-object finding.

After opening the gate once for the whole release, run the canonical browser test in both configured Playwright projects:

```sh
bun run test:e2e:dev -- tests/e2e/shared.attachments/attachments-v2.spec.ts --project=desktop
bun run test:e2e:dev -- tests/e2e/shared.attachments/attachments-v2.spec.ts --project=mobile
```

Finally, verify HTTP 200 and inspect the attachment readiness body at every required endpoint:

```sh
curl -fsS http://127.0.0.1:3100/api/health
curl -fsS https://dev-erpx.jesadakorn.com/api/health
curl -fsS http://127.0.0.1:3103/api/health
curl -fsS https://erpx.jesadakorn.com/api/health
curl -fsS http://127.0.0.1:3101/api/health
```

Health checks are not browser acceptance. Use the existing authenticated session to inspect affected Leave, Procurement, Inventory, Sales, record-document history/drawer, sensitive-document denial, and public-media approval/revocation behavior on both public hostnames.

At closeout, representative Leave, Procurement, Inventory, E-commerce, and Support URLs routed correctly through both public hostnames and returned HTTP 200 at the expected console sign-in destination for an unauthenticated request. Chrome still has ERPX tabs whose titles indicate an authenticated workspace, but macOS is locked and computer-use could not focus or inspect them. No credentials were guessed, no lock was bypassed, and authenticated attachment-panel appearance remains intentionally unclaimed until the operator unlocks the Mac and the gate is approved to open.

## Genuine remaining blockers

1. **The frozen reconciliation is not gate-ready.** H001/H002 retain 36 unknown-purpose fixtures, 107 unavailable records in total, 36 missing ledger entries, and two quota deltas; Billing retains 23 orphan objects. The subsequent read-only audit exposes 133 deduplicated adjudication items but does not resolve them. All current blockers are provenance-confirmed synthetic fixtures and their forward generators are fixed, while the persisted records remain deliberately preserved. Cross-Company, object-presence, and checksum checks are clean. An operator must explicitly approve their retirement/reset or another policy-supported outcome; do not relabel or delete them for rollout convenience.
2. **Authenticated browser acceptance is not complete.** The canonical E2E file covers the shared Leave surface on desktop/mobile but correctly skips while the gate is closed. Existing authenticated ERPX Chrome tabs could not be inspected because macOS remained locked. The required authenticated inspection across Procurement, Inventory, Sales, sensitive documents, portals, and public publication still has to run after gate approval and after the operator unlocks the Mac. Keyboard/screen-reader and publication/revocation acceptance are not established by the current closed-gate run.
3. **Staging and production deployment remains an operator action.** Both still run an older release. Apply the schema, runtime services, reconciliation, gate change, and browser/health checks independently in each environment; never reuse a development or staging reconciliation receipt.

## Deliberate non-blockers

- Generic bound files have no automatic deletion without an approved retention policy. Legal uses the versioned `legal-managed` review policy; conservative preservation elsewhere is the intended behavior.
- OCR/full-text attachment search remains Legal-only.
- Generated receipts, reports, payroll/payment exports, and similar artifacts remain generated artifacts; reconciliation knows their object keys, but they are not user-upload File Links.
- A photographed signature remains ordinary evidence, not an electronic signature or OTP.
- Anonymous upload, raw public originals, cross-Company link reuse, and automatic attachment-byte delivery to AI remain unsupported by design.

## Release conclusion

The old “platform foundations only” conclusion is obsolete: the central lifecycle, broad owner inventory, domain binding, specialized compatibility, sensitive authorization, shared UI, migration/reconciliation tooling, rollback, and private/public boundary are implemented in the working tree.

The correct current decision is evidence-based: **use the 133-item read-only manifest to obtain an explicit approved disposition for the preserved synthetic blockers, execute that decision without bypassing audit or lifecycle rules, repeat the frozen final reconciliation to `gateReady: true`, and complete authenticated browser acceptance before activating the gate in development.** Staging and production remain on their older release until the same migration, reconciliation, health, and browser checks are performed independently. Until then, keep the global gate closed.
