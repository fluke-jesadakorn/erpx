# Field visit Files capability boundary

This is the bounded Rust capability boundary. [PR #8](https://github.com/fluke-jesadakorn/erpx/pull/8) merged candidate `e4be8bddb0d499fe137fccb5e8e487a5895ad469` into `codex/rust-backend-migration` at `15a4cf28c99df361a2fe0cb2535c960bd3fb2b5f`. Docker support continues on `codex/field-files-docker`; the authorized integration target remains the migration branch. No complete catalogue workflow or owner is closed by the component evidence below.

## Scope and ownership

The candidate covers the four `field_service_visit` record purposes: `site_photo`, `technician_report`, `signed_job_sheet`, and `exception_evidence`. It includes set list/create, reservation, quarantine publication, upload completion/replay, status, original/preview reads, withdrawal, binding, native scanning, and the required draft/orphan recovery operations. Field, Identity, Commercial, Manage ERPX, Records, and Workflow provide their existing public owner interfaces; Files owns its metadata, bytes, scan attempts, and storage ledger.

Persisted Field markers select the native family, including ambiguous, orphaned, and historical markers. A request label, configured ID list, or native transport failure never transfers a Field operation to TypeScript. Rust independently verifies coherent lineage and current authority. Non-Field Files consumers continue to use the shared implementation.

Each Holding explicitly registers the Companies serviced by its native workers. The current Account selection supplies the Company; the former bootstrap Company is not a grant. An unconfigured selected Company fails closed before reading an upload body. Current identity, source version, permissions, purchases, and transaction identity are checked again inside the relevant unit.

## Transport and policy

The Next BFF retains its public Files paths and forwards the captured rendered context and normal opaque session to private Rust. The native family paths are:

| Operation | Private path below `/api/v1/holdings/{slug}/field-service` |
| --- | --- |
| List/create sets | `/attachment-sets` |
| Reserve | `/attachment-sets/{id}/files` |
| Upload | `/uploads/{id}/content` |
| Status | `/attachment-links/{id}/status` |
| Read bytes | `/attachment-links/{id}/content?variant=original` or `preview` |

Bind and withdrawal use the existing native `/attachment-sets/{id}/bind` and `/attachment-links/{id}/withdraw` contracts. JSON family responses retain `{data: ...}`. Native Rust generates the [intake/upload/status OpenAPI contract](field-service-attachment-intake.openapi.generated.json) and TypeScript DTOs.

Selected authority precedes request-body and content-query interpretation. Set JSON is bounded to 2 MiB, reservation JSON to 64 KiB, and upload bytes to 10 MiB. The BFF reads JSON once and cancels oversized streams. Binary responses carry the exact byte checksum and bounded RFC 5987 UTF-8 filename; previews use sandbox headers. UI byte reads carry the rendered context and revoke object URLs when context changes.

The private configuration requires an explicit `fieldFilesPolicy` with `mode` (`enabled`, `read-only`, or `disabled`) and `uploadsFrozen`. Provider targets contain explicit Holding/Company IDs, exact registered files bindings, canonical private store/temp directories, and validated local scanner/preview tools. There is no ambient provider-path or production fallback. Disabled intake returns the retained disabled behavior; read-only exposes only retained clean bound evidence; freeze rejects intake while permitting otherwise authorized reads.

## Publication, jobs, and retirement gates

Reservation preserves the original command key and normalized metadata. Files owns short authorized database units around immutable filesystem publication. Publication pins, object locks, exact receipts, current clock checks, and retained cleanup claims prevent a losing request or stale worker from deleting another upload. Content uses authorization before private I/O and a fresh authorized unit afterward. Denied-content audit is best effort in a separate fresh unit and preserves the original denial.

Native scanners exclusively claim persisted Field sources; the legacy scanner excludes them at claim and settlement. Legacy upload and cleanup units use pre-transaction object locks for irreversible I/O and retain the normal owner-source locks, purchase scopes, and deferred-effect checks. Their callback cannot replay a physical write. These are fences; they do not by themselves establish removal of the replaced implementation.

The native draft/orphan cleanup worker is mounted with checked transaction phases. Targeted component proofs cover cancellation guards, staged-publication crash recovery, binding changes, and reference races; they do not claim a real process-kill recovery test. It must retain bound/historical evidence, audit, migration history, receipts, and publication history. Retention review, legal preservation, and destruction of retained sources are separate capabilities; they must stay explicitly parked under native ownership rather than silently falling back to legacy deletion.

The same-database business/history regressions and immediate Field-only legacy retirement have passed targeted verification. Final local checks passed and the scoped PR merged; Docker and Dev Container qualification remain the next delivery gates. Its Docker and Dev Container support follows that accepted Rust capability. The shared Count image and contributor environment already delivered do not qualify the new Files runtime.

## Current evidence limits

The new provider fixture uses real Account/SSO, leased PostgreSQL, an owned loopback HTTP listener, ClamAV, the preview binary, and the native worker entrypoint. It verifies quarantine denial, available bytes/checksum, bind/withdraw replay, policy modes, and Company denial. Separate tests cover legacy/native claim races, inherited Expense and Leave checks, pin/cleanup races, and pure frontend uncertainty semantics. An isolated working-snapshot browser journey also passed normal Account sign-in, Company/role selection, actual upload/scan/bind and original download with matching checksum, with no page or console errors. SQL reconciliation confirmed the exact receipts and stored ledger. This is not immutable-candidate acceptance: final source reconciliation, post-retirement browser checks, and independent review remain required.

Real deferred-trigger faults exposed a finalization ordering bug: an access-audit trigger could mutate the customer after the Field witness check. Allowed and denied access finalization now flush deferred constraints before final source/authority/evidence checks. The actual deferred source, IAM and denial-audit faults now pass, including rollback of altered denial evidence while preserving the original denial response. Mixed purchase, compatibility binding, withdrawal and no-charge/posted-cost cases use the same verified native lease for their original business lineage and native Files operations. A registered Rust owner proof uses real store/source counters and an actual one-shot serialization failure to retain the former zero-read/no-retry/one-read-on-finalization-retry assertions under the explicit two-phase architecture.

Hosted quota-exhausted CI is not reported as passing. Source-bound local verification is the authorized merge gate. Keep the complete Field checklist and the 206-workflow/41-owner counts separate from these capability checks.


## Scoped delivery checklist

This checklist is separate from the full Field 18-item checklist and the 206-workflow/41-owner registers.

- [x] Native contracts and all four Field visit purposes use the persisted family owner.
- [x] Reservation, publication, real scanner/preview, reads, bind/withdraw and draft/orphan worker recovery are implemented and component-verified.
- [x] Authorization, Company isolation, transaction/receipt replay, deferred source/IAM/audit faults and cross-Company concurrency checks pass.
- [x] Normal Account/SSO and selected Company/role lead to actual upload, scan, binding and checksum-matching download; post-change cold visit and explicit warm chooser/readback pass.
- [x] Original purchase, compatibility, withdrawal/history and no-charge settlement assertions run with native Files on the same leased database; native owner tests retain physical-I/O and retry observations.
- [x] Replaced Field-only TS implementation, selectors and unused dependencies are removed; shared consumers remain explicit.
- [x] Final frozen-candidate local checks and independent review are recorded.
- [x] Scoped PR is merged into `codex/rust-backend-migration` and remote SHA verified.

The initial fresh browser run recorded a one-time role-chooser React warning and two resource404 messages without their stacks/URLs. The explicit warm chooser and isolated cold visit did not reproduce them. Do not call that original console clean, infer whole-Field hydration acceptance, or discard its evidence. The complete Field Company-selection gate remains separate.

## Retired implementation and retained consumers

The four `server-field_service` adapter/public pairs `attachment-purchases`, `attachment-read`, `attachments-policy-facts` and `attachments-service-facts` are deleted. Their exports and the unused TS Files↔Field workspace dependencies are removed. Shared Files no longer contains the Field source/version/purchase implementation; direct Field reads and all mixed persisted Field markers fail closed. The guard runs inside locked content callbacks and borrowed metadata units, including alternate Portal ingress. Public status/list classification and metadata share one serializable snapshot. No TS fallback is available when native transport fails.

Unused presenter/client ID-selector props, public selector APIs, optional runtime selector wrappers and the obsolete external bind configuration are removed. The retained bind/withdraw bridge factories transport the already-selected resource with exact rendered context; they grant no authority and do not implement business rules.

Shared Files application/repository, scanner and cleanup code still serves non-Field owners and excludes Field-owned resources. The static Field policy rows in `packages/server-files/src/domain/attachments/policy.ts` remain declarative inventory for the engineering matrix, not an authorization or writer implementation. Remove those rows only when that inventory consumes the Rust contract directly. Remove other owners' shared logic only with their own verified migrations. Database rows, audit, receipts, migrations, publication history and earlier evidence remain intact.

The source-bound [local verification receipt](../../testing/evidence/2026-10-31-field-visit-files-native.json) records the changed-source manifest and evidence limits.

The completed Rust capability checklist is **8/8**. Its branch-to-merge wall interval was 21,822 seconds (2026-10-30 18:43:26 to 2026-10-31 00:47:08, Asia/Bangkok), including waits; this is not an active-engineering-time estimate. The full Field checklist remains 7/18, workflows remain 13 partial/0 verified/0 merged of 206, and owners remain 0/41.
