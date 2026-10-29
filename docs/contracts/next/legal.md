# Legal module: implementation and acceptance

Updated 2026-10-21. The development implementation covers the fourteen groups below. Production acceptance remains separate: a legal reviewer must approve jurisdiction rules/templates, signatory authority, trust material and retention policies; external email and AI require configured providers. No real agreement was signed or external invitation sent during development verification.

## Entry points

Staff use `/holdings/[holdingSlug]/legal` and `/api/v1/holdings/[holdingSlug]/legal`; existing workspace resolution supplies Company, Branch and Department. External users may use `/legal-portal/[holdingSlug]`. Current migration acceptance covers scoped reads and revocation of shared document versions, plus private-document downloads and access-history evidence, for identities already authorized to access the Legal Portal. New external-recipient invitations and enrollment, invited-account verification and acceptance, invited-signer fresh MFA, consent pinned to an exact document version, and signing evidence are deferred together as a separate feature blocker; the Portal signing route remains disabled.

Implementation lives in [the Legal server owner](../../../packages/server-legal/src/) and [Legal feature UI](../../../packages/feature-legal/src/), composed by `apps/legal` and `apps/legal-portal`. Additive holding migrations establish typed aggregates, workflows, immutable evidence and processing. Legacy preview data remains intact; only explicit reviewed imports create live records. The Legal Portal invitation and enrollment feature is outside current migration acceptance.

## Capabilities

| Group | Implementation |
| --- | --- |
| Work centre | ACL-filtered registers/counts/search, assigned Work and approval tasks, Calendar dates, SLA/reminder jobs, operational reports |
| Requests | Typed requests/advice, configurable independently approved intake forms, validation/SLA, assignments, information request/response, reopen/close and conversion |
| Contracts | Lifecycle and parties/value/jurisdiction, shared sequential Approval adapter, pinned documents and signer metadata, authority records, linked amendment/renewal drafts |
| Drafting | Versioned templates/clauses/variables, section editor, clause discussion, immutable drafts, word redlines, DOCX import and actual Thai DOCX/PDF export |
| Signing | No signature is sent or recorded in this candidate; current signing commands fail closed with `LEGAL_SIGNING_DISABLED`. Legal Portal invitations/enrollment, invited-signer fresh MFA, exact-version consent and signature evidence are a separate deferred feature blocker. |
| Obligations | Typed commitments and schedules, delivery/payment/guarantee/insurance fields, owners/dates/evidence, configurable reminders and escalation |
| Cases | Court/parties/claims/exposure/strategy plus separate hearing, filing, order, appeal, settlement and enforcement workflows |
| Compliance/privacy | Requirements/controls/findings/remediation, policy acknowledgements, processing activities, DSAR and breach workflows with evidence |
| Licenses/corporate | Company references, license conditions/renewals, powers/resolutions/minutes, independent evidence-backed signing authorities with dates/currency/limits |
| IP | Rights/owner/country/classes/application/renewal/licensing/infringement registers and prosecution/renewal/enforcement tasks |
| Evidence | Immutable private versions and hashes, text extraction/Thai OCR durable queue, original attachment preservation, custody workflow, access/export logs |
| Hold/retention | Typed cyclic-safe impact propagation, overlapping holds, custodian notices/acknowledgement, independent release, approved retention and destruction journal/certificate |
| Portal/counsel | For already-authorized external identities: scoped shared-version viewing and revocation, private downloads and access history, discussion/revisions, conflict/time/fee/invoice workflows and Finance purchase-request handoff. New Legal Portal invitation/enrollment and invited-signer fresh-MFA, exact-version consent and signing remain a separate deferred feature blocker. |
| AI/reports | Company opt-in, durable proposals with immutable citations and current ACL checks, human acceptance, knowledge search, Thai draft starter pack, SLA/cost/access/job reports |

Registers support filters, pagination, saved views, validated import preview/atomic commit, permission-controlled export and history. Imports accept up to 100 rows; exports up to 10,000. Exports are audit representations, not round-trip import files. Specialized child workflows have search/pagination, optimistic versions and immutable history included in evidence bundles.

## Security and evidence invariants

- Holding/Company/organizational scope and explicit confidential-matter membership apply to lists, counts, reports, search, AI, Work, Calendar and downloads. Administrative roles do not bypass matter membership.
- Staff commands combine idempotency receipts, current IAM/context checks, audit and outbox in one transaction. Portal commands also record receipts, audit and outbox. Internal discussion is never returned externally.
- Shared Approval routes pin a source digest, immutable document hash and ordered signer metadata. Independent reviewers cannot approve their own material work. Generic approval endpoints cannot bypass Legal policy. Signatory authority records remain distinct from contract approval; no signing command is currently active.
- A future ERPX signing workflow must retain electronic consent evidence and keep the original separate; it must not claim a certificate-backed PDF signature without one.
- PyHanko validates integrity, certificate identity/dates, chain, revocation and timestamp independently. Missing trust/revocation evidence yields `indeterminate`, never a fabricated pass. Certificate validation makes no certificate-supplied network requests.
- AI has no approval/signing/payment tools. Sources are reauthorized, pinned by version/hash and treated as untrusted text; accepted suggestions become human-reviewed draft evidence.
- File mutation uses a common advisory lock. Generic overwrite/delete cannot remove Legal files or preserved held originals/derivatives. Destruction requires an independently approved policy/request, no active hold, a committed execution journal and a fixed manifest. Interrupted execution resumes; evidence certificates and audit remain. Automatic destruction is off.
- Rule/template revisions create new drafts. Impact views identify accessible linked records without silently rewriting existing deadlines. Deadline helpers use explicit reviewed inputs, timezone and supplied holiday calendars; they infer no statutory limitation period.

## Setup and operations

Runtime target: Node.js 26.11.1 and Bun 1.4.3 with PostgreSQL, private files and durable jobs. The installed private production service remains on Node.js 24.20.0 until a reviewed release is deployed. Run `bun scripts/legal/setup.ts` to install the pinned Python validation environment, verify official English/Thai OCR model hashes and execute signed-PDF fixtures. Poppler and Tesseract must be installed by the operator. Executables run without a shell with bounded input, output and timeouts.

Configure absolute persistent paths in deployment: `ERPX_LEGAL_PYTHON`, `ERPX_TESSDATA_DIR`, `ERPX_TESSERACT_BIN`, `ERPX_PDFINFO_BIN`, `ERPX_PDFTOPPM_BIN`, `ERPX_PDFTOTEXT_BIN`; OCR defaults to `eng+tha`. See `.env.example`. Release builds include validation scripts and a bundled DOCX extractor; Python environments and OCR data are operator-managed dependencies outside the release directory. Documents are limited to 10 MB; scanned PDF OCR is limited to 25 pages.

Company managers propose trust roots/intermediates/CRL/OCSP; a different authorized reviewer approves them. The same independent-review principle applies to intake forms, authority and retention policy. Trust material is explicit and versioned, not an implicit browser trust store.

A separate jobs worker runs Legal processing, invitations, reminders/expiry, retention and AI with independent error isolation. Restart it after worker code changes; Next hot reload does not reload workers. Processing has durable leases and interrupted-job recovery. Invitation jobs retry with backoff; absent email configuration records failure. Operational reports expose queue status without confidential payloads in logs. Configure the existing email and AI providers before live use.

The Thai starter pack contains six official-source review procedures and four original draft templates for general business, confidentiality, hotel/events and retail. Import creates drafts only. Effective dates, applicability, review notes and independent acceptance are required before activation. Government filing/payment APIs and external e-sign provider APIs remain outside this release as agreed.

## Verification and rollout

> Recorded observations apply to their stated date/environment. Current coverage and remaining acceptance are in the [verification inventory](../../status/capability-delivery-matrix.md); old gaps do not override current owner behavior.

Automated coverage includes organizational/confidential ACLs, stale writes/replay, shared sequential approvals, the disabled-signing boundary, signatory authority metadata, immutable versions, all ten specialized workflow families, overlapping/cyclic holds, protected file paths, controlled destruction, imports, DOCX/PDF, scoped portal reads/downloads and their access audits, governance/SLA, report/knowledge ACLs and reviewed AI acceptance. Existing invitation/enrollment integration cases remain regression coverage; they do not count as current Legal Portal migration acceptance. These cases do not attest to a staff or Legal Portal signature. Python fixtures cover trusted, unknown-revocation, revoked, expired and untrusted certificates. Native backup/restore integration was exercised.

The recorded pre-Holding browser checks used the existing authenticated session at `https://console-dev-erpx.jesadakorn.com/holdings/acme/legal`. The fictional request and draft document are development fixtures; request information-response and report tables were exercised. No real external user was impersonated and no legal acceptance was fabricated.

Live rollout still requires legal-staff UAT and Company-specific trust/authority/retention review. Failed uncommitted uploads can leave inaccessible private orphan bytes; no automatic orphan deletion is enabled. Full disaster-recovery and external-provider operational acceptance must be performed in the intended deployment environment. Legal Portal invitation/enrollment, invited-account verification/acceptance, fresh-MFA consent pinned to an exact document version and signing evidence are excluded from this migration's acceptance and remain a separate feature blocker; the migration's Portal gates apply only to already-authorized identities. No external e-sign provider is included.

### Recorded verification snapshot, 2026-09-19

- Pinned Node 24.20.0 / Bun 1.2.17: typecheck passed; lint passed with three existing warnings outside Legal.
- Legal and deadline integration tests: 33 passed, including sequential approval, changed-source rejection, revoked signing authority, external billing isolation/replay, Finance handoff/native references and AI policy/citation failures.
- Legal/migration/Work-cache focused regression: 45 passed before the final additional AI test.
- Python certificate validation: 5 scenarios passed; English and Thai OCR assets passed pinned checksum verification.
- Authenticated public browser E2E: fictional request/document persistence, information response, workflow filters, report data and retention defaults verified.
- Local and public health checks: HTTP 200; detached development server remains on 3100 with a separately rebuilt Legal jobs worker. Production on 3101 was preserved.
- Public UI durable-processing smoke: queued extraction of the fictional draft completed through the rebuilt worker in one attempt, returned the correct Thai/English text and immutable checksum, and required human review before creating a new draft.
- Full integration suite on pinned Node: 79 test files passed, 2 skipped; 726 tests passed, 3 skipped, no failures (526.84 seconds). Provider-dependent skipped cases are not counted as verified.
- Final isolated release build passed: `.runtime/releases/2026-09-19T04-34-28-745Z`. No production cutover was performed.
