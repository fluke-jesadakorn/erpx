# Attachments and record documents

ERPX accepts private files only for real, module-owned workflows registered in the server attachment policy registry. A File Source contains immutable original bytes; a File Set groups one record or action purpose; a File Link supplies the target-specific authorization, retention, and lifecycle. Unknown target, purpose, or component combinations fail closed.

## HTTP contract

All endpoints are relative to `/api/v1/holdings/{holdingSlug}`, require an authenticated work context, use private uncached responses, and check the application Origin for mutations. The mutation surface is unavailable while `ERPX_ATTACHMENTS_V2` is disabled. A rollback may set `ERPX_ATTACHMENTS_READ_ONLY=true` while leaving the main gate off; in that mode only clean, already-bound evidence remains readable and reserve, reuse, withdrawal, publication, and bind mutations remain closed.

| Method / path | Contract |
| --- | --- |
| `GET /attachment-sets?targetType=&targetId=` | Return only sets and links currently visible through the target policy |
| `POST /attachment-sets` | Create or recover a set from `{ targetType, targetId, componentKind, componentRef?, purposeKey, expectedTargetVersion }` |
| `POST /attachment-sets/{setId}/files` | Reserve quota from `{ filename, mediaType, bytes, caption? }`; returns `uploadIntentId`, `attachmentLinkId`, `uploadUrl`, and `expiresAt` |
| `PUT /uploads/{uploadIntentId}/content` | Stream the declared bytes into quarantine; returns `202` with `scan_pending` for v2 intents |
| `POST /attachment-sets/{setId}/links` | Create a same-Company link to a source the actor can currently read |
| `GET /attachment-links/{linkId}/status` | Return the safe lifecycle and scan state, never scanner internals or hidden target metadata |
| `GET /attachment-links/{linkId}/content?variant=original\|preview` | Reauthorize the current target, audit the attempt, download originals, and render only server-generated previews inline |
| `POST /attachment-links/{linkId}/withdraw` | Withdraw a draft link with a required reason before it is bound |
| `POST /media-publications` | Propose publication of an eligible private image link with alt text, order, and expected revision |
| `POST /media-publications/{id}/approve` | Independently approve the sanitized derivative; a public route never exposes the original |
| `POST /media-publications/{id}/revoke` | Revoke an approved derivative with a reason while retaining the immutable source and publication audit |
| `POST /media-publications/order` | Reorder approved derivatives with an optimistic publication-order version |

Business commands accept `attachmentSetIds` and bind available links in the same transaction as the record transition. Upload completion alone never changes the business record version. Existing source-ID upload and download routes remain compatibility adapters for one release and must resolve a currently authorized link before returning bytes.

## Lifecycle and limits

- An upload reservation lasts 15 minutes. A clean but unbound draft expires seven days after scanning; withdrawal requires a reason and follows the same cleanup boundary.
- A bound link cannot be changed or deleted. Corrections and superseding record documents create new links while retaining history.
- Generic sets accept JPEG, PNG, WebP, HEIC, HEIF, and PDF up to 10 MiB per file. The default is 10 files per set; an explicit policy may set 1–100.
- Messaging and Expense retain their reviewed CSV exceptions. Legal retains PDF, PNG, JPEG, and DOCX. Specialized formats never make those types valid for a generic purpose.
- Original images keep their checksum and bytes. Inline previews are decoded, bounded, normalized, and stripped of EXIF/GPS metadata. PDF originals download; page previews are generated on the server.
- A source is readable and bindable only in `available` state. Scanner failure, stale signatures, malware, or an unknown legacy owner fail closed.
- A photographed or scanned signature is ordinary evidence only. It is not a formal electronic signature, approval credential, or OTP substitute.
- ERPX does not automatically send attachment bytes to an AI provider. A workflow may expose a reviewed metadata manifest, but dereferencing file content requires a separate explicit product and policy decision.

## Authorization and retention

- The server derives Company, classification, audience, retention, and policy revision. Clients cannot supply or weaken them.
- Cross-record reuse requires current read access to the source and attach authority on the destination. Both targets must belong to the same Company. Legal preservation is the only byte-copy exception.
- Access to an original, preview, portal link, or denied guessed identifier is append-only audited. Metadata is not disclosed when the target itself is hidden.
- Bound data is not automatically destroyed without an approved versioned policy. Every active link and Legal Hold must permit destruction; the strictest and longest obligation wins.
- Authenticated customer or legal portals may upload only to an explicit owner and purpose. Anonymous generic uploads are not supported.
- IAM, migration, release-acceptance, and provisioning purposes accept only sanitized review evidence. Filenames, captions, and the reviewed manifest reject password or credential exports, private keys, secrets, configuration dumps, and database dumps. Binding requires a fresh server-recorded sanitization attestation, invalidated whenever the manifest changes.

## Finance, hospitality, time, Organization, and system inventory

This inventory lists only live table-backed owners. Finance ledgers/reports and Today/Executive projections remain source links and are not attachment owners.

| Owners | Registered purposes | Binding surface |
| --- | --- | --- |
| `finance_journal`, `finance_bank_statement`, `finance_accrual_schedule`, `finance_asset`, `finance_lease`, `finance_loan`, `finance_revenue_contract` | `journal_support`, `bank_statement`, `accrual_support`, `asset_support`, `lease_support`, `loan_support`, `revenue_support` | Record-document bind; journal, bank-statement, and accrual consoles expose the shared document drawer. Owners without a current read/detail surface remain API-policy only rather than gaining a placeholder page. |
| `finance_bank_transaction`, `finance_period`, `finance_payment`, `finance_refund_obligation`, `finance_supplier_batch` | reconciliation/close workpapers, payment/refund proof, refund/batch support | Bank match, close approval, payment confirmation, refund confirmation, and supplier-instruction confirmation bind action evidence in the same business transaction. Refund and supplier support also use record-document panels. |
| `pos_sale`, `pos_refund`, `pos_shift` | `void_evidence`, `refund_evidence`, `cash_variance` | `sale.cancel`, `refund.approve`/`refund.reject`, and `shift.review` only. |
| `pms_guest`, `pms_reservation`, `pms_stay`, `pms_task`, `pms_lost_property`, `pms_property` | identity/consent, voucher, registration/transfer, room/damage/lost-property photos, handover, OTA reconciliation | `task.complete`, `lost.release`, and `ota.settle` bind action evidence atomically. Typed record purposes use the canonical record-document API; no synthetic PMS owner is created. |
| `overtime_request`, `time_entry` | planned/actual/exception overtime evidence; corrected-entry proof | Overtime submit/actual and rejected time-entry resubmission bind atomically. |
| `organization_node`, `organization_change` | registration, licence, branch permit, organization-change proof | Versioned record/action owners with Company and Branch scope checks. |
| `iam_review`, `iam_migration`, `company_release_acceptance`, `people_provision_operation`, `company_provisioning` | sanitized review, migration, release, and provisioning evidence | Requires a fresh server-recorded sanitization attestation before bind; sensitive filenames, extensions, and labels fail before upload reservation. |

## Medical leave evidence

Leave uses `medical_certificate` and `leave_supporting_photo`. Only an available medical-certificate link satisfies a versioned sick-leave rule. A request may be submitted before the certificate arrives, but final HR approval is blocked until the required current manifest is reviewed and pinned.

The requester, the current eligible approver, an approver with a recorded decision, and a user with `leave.evidence.read` may read medical evidence. A person removed before recording a decision loses access immediately; general People or manager access is insufficient.

## Support, customer portal, and collaboration

Support uses separate `internal_evidence` and `customer_shareable` action purposes. The current eligible case owner or Support manager may bind internal evidence; only an eligible Support manager may bind the customer-shareable purpose. Binding increments the case version and freezes the clean manifest in the same command transaction. An authenticated customer whose approved order binding is still current receives only a label, link identifier, and sanitized preview for customer-shareable links. The portal never returns the original, checksum, byte count, source identifier, internal filename, or files from the internal purpose. Revoked identities, expired bindings, guessed links, another Company, and unavailable source orders fail closed.

Customer uploads use `/api/customer-portal/{holdingSlug}/attachments/...`, require a verified signed-in identity, and are limited to `customer_access_binding/customer_support_evidence/action` for the explicit approved binding. There is no anonymous or generic portal upload. The resulting clean set binds atomically only with `request_support`.

Incident evidence binds only with Start, Update, or Propose mitigation. Task evidence binds only with Submit, Save progress, or Complete, and current task/delegation authorization is rechecked during the transaction. Published conversation files use `thread_file`; handovers use the more restrictive `handover_file`. Both remain attached to the source conversation and reauthorize its audience on every read. Today, Calendar, and Executive projections expose source links only and never own file sets.

## Sensitive workforce documents

Sensitive workforce files never inherit the broad `records.*` or `people.manage` permissions. The closed policy registry uses the dedicated `people.documents.*`, `payroll.documents.*`, `talent.documents.*`, `learning.documents.*`, and `benefits.documents.*` permissions together with the exact subject, workflow assignment, purpose, and record scope. Benefits review uses `benefits.review`; it does not make internal Benefits documents visible.

Employee self-service returns only typed purposes explicitly marked self-visible, such as a contract, certificate, benefit booklet/election document, or the employee's own submitted evidence. Evaluation, investigation, background-check, candidate-only, payroll-internal, and HR-memo purposes return neither content nor metadata to the subject unless a separate policy explicitly allows that exact type.

Retirement-clearance evidence is task-scoped. A department permission is necessary but does not grant visibility by itself: only the assigned colleague or the colleague recorded as completing that task can read its evidence. Other case viewers receive no evidence metadata or completion note for that task.

## Operations

The attachment gate may open only when the local `clamd` Unix socket and signature updater are healthy, definitions are no older than 24 hours, all legacy files are scoped or explicitly quarantined, the scan backlog is empty, and object/checksum/quota reconciliation passes. Rollback disables new reserve and bind operations while preserving clean bound reads and all audit history.

The one-release compatibility adapters use that scanner even while the global UI gate is closed. `/api/health` therefore probes scanner and preview readiness unconditionally, requires the jobs process to report its own fresh readiness with the same upload-freeze setting, and distinguishes Billing scan compatibility from the final reconciliation receipt required to open the gate. The ordered native-host procedure is [the attachment rollout runbook](../../../ops/macos/attachment-rollout.md).

Platform Billing remains in the control database, but uses the same 10 MiB limit, quarantine/scan states, access audit, and readiness contract. Its separate storage ledger counts an original, preview, or publication derivative once; creating another logical link never consumes the physical bytes again.
