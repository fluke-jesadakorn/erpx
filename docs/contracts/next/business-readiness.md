# Business readiness and release acceptance

Delivery scope is governed by [the production plan](../../production-plan.md), with Company release criteria in `src/data/production-scope.ts` and full-catalogue engineering scenarios tracked separately. The 187-capability catalog is reference inventory, not the Company release acceptance set.

## Independent meanings

- `accept` is an action whose meaning comes from the named object and command. `accepted` is that object's recorded outcome; neither word grants release approval by itself.
- Company setup acceptance (`company_business_acceptance`) concerns Company identity, organization and staff placement. Its existing API and `submitted/accepted/revoked` lifecycle remain compatible.
- Workspace observations (`business-acceptance-evidence.json`) are checksum-verified evidence with a stated environment and limited scope. They never promote gates.
- Capability reviews (`capability-gate-reviews.json`) are the durable source for generated inventory gates. Generator runs preserve explicit reviews and validate vocabulary, reviewer, evidence, timestamp and duplicates. They do not certify a Company release.
- Company release acceptance is stored in PostgreSQL, not generated JSON. Every criterion in the current scope requires independent review. A review attests the submitted evidence; a recorded checksum alone does not establish the correctness of its contents.

The compatible `implementation.status` field records dated, scoped inventory evidence; provider labels and Company release decisions are independent. The [verification report](../../status/capability-delivery-matrix.md) separates these axes and retains the original engineering source/candidate. The capability inventory's **Flow accepted** label means an end-to-end workflow has passed in the system with the required evidence. It is a software capability status, not Company Setup Acceptance or Company Release Acceptance. A release criterion becomes `passed` only through its authorized, independent evidence review. The overall release is accepted only after every required criterion passes for the submitted Company, build and configuration.

Gate vocabulary is `pending / passed / not_applicable`. Only a provider **inventory** gate can be not applicable, with a reason. All current release criteria are required. `companySignOff` on the existing readiness response retains its old **setup-only** meaning for compatibility; clients should use explicit `setupSignOff` and `releaseSignOff` fields. The new `releaseAcceptance` field exposes scope/build/configuration, criteria and blockers.

## Routes

All paths below start with `/api/v1/holdings/{holdingSlug}/business-readiness`.

| Method / path | Authority and behavior |
| --- | --- |
| `GET /` | Existing Company organization-management readiness view; additive fields above |
| `GET /releases` | Latest release plus current inputs and blockers; active Company release-review assignment |
| `GET /releases/{id}` | Same Company only; historical record, evidence and effective gate states |
| `POST /releases` | `organization.manage`; submit immutable evidence for all criteria |
| `POST /releases/{id}/review` | Criterion-specific Company permission; independent person; expected version |
| `POST /releases/{id}/revoke` | `organization.manage`; expected version; keeps evidence and events |

Commands require same origin and `Idempotency-Key`. Request context resolves Company; input cannot choose another Company. Branch/Department reviewers are rejected. Permission and assignment validity are rechecked in the write transaction. Linked login identities of the same person cannot independently approve. Replays revalidate authority and require the original request fingerprint. Concurrent decisions against one version produce one event.

Submit payload: `scopeVersion`, `softwareVersion`, `expectedConfigurationRevision`, `expectedSetupVersion`, `reason`, and `evidence`. Evidence has exactly one item per criterion: `{criterionId, summary, files:[{reference, sha256}]}`. References identify independently reviewed retained artifacts; the API stores them and does not fetch arbitrary URLs. Raw salary data or credentials must remain in authorized source storage, not copied into release descriptions.

Review payload: `{expectedVersion, criterionId, state: "passed" | "pending", reason}`. Returning a gate to pending withdraws that decision. Changing submitted evidence requires a new release. Revoke payload: `{expectedVersion, reason}`. Error classes include 403 authority/independence, 404 Company-scoped not found, 409 stale input/version/idempotency conflict and 422 invalid payload.

## Binding and invalidation

The isolated build stamps a source-derived `softwareVersion` in its private build tree. It hashes the isolated snapshot of application source, public assets, worker/build scripts, PostgreSQL migrations, dependency lockfile, build configuration, pinned toolchain and public build environment values. Web and worker bundles use the same stamped snapshot. No `.env`, secrets or database credentials enter the artifact. Development source has `unbuilt` and cannot submit production acceptance.

A release binds Company, scope version, software version, setup acceptance version, material configuration revision, immutable evidence SHA-256 and requester. Review records retain reviewer assignment, required permission, reason and time; immutable events retain every change. Effective acceptance also checks that reviewer authority remains active and independent.

Migration `0304` adds these records without modifying old setup/evidence rows. Configuration revision triggers cover organization nodes, setup acceptance, charts/accounts, Commerce settings, POS settings/registers/catalog, pay groups, country packs and PMS property/room/rate configuration. PMS business date/property operational state and ordinary transactional records do not invalidate acceptance; changing room metadata conservatively requires re-review. Shared country-pack changes invalidate all existing Company revision rows. New configuration surfaces must join this invalidation contract before they become releasable.

Review/activation serialize against configuration revisions. A new build/scope/configuration, revoked setup/reviewer or changed establishment count makes release eligibility pending. All required criteria must pass for the latest release. A second submit does not inherit old gate decisions; historical releases report a superseded blocker and cannot receive new reviews.

## Rollout

Feature rollout remains separate (`off / pilot / available`). `pilot` denotes an explicitly activated rehearsal, not production sign-off. `available` requires the current complete release. Existing shared API, owner, AI-result and scheduled predicates reject an available rollout whose release becomes stale or revoked. Generating inventory JSON cannot enable a rollout. Module-level inventory acceptance is reported separately as `inventoryAccepted`.

Thai payroll is required engineering delivery. Production-mode runs require the recognized source-dated rule pack and independently approved employee statutory evidence; readiness checks the actual adapter/results rather than blocking all runs. [Payroll](payroll-core.md) defines these guards and unsupported categories. The recorded full-catalogue engineering run does not grant statutory certification or Company release acceptance. The public development Tunnel is not changed by acceptance APIs.

## Validation

`tests/release-acceptance.test.ts` covers persistence, migration reapplication, independent review, cross-Company reads, revoked authority, stale/concurrent versions, replay conflicts, immutable evidence, all-criterion completion, configuration invalidation and shared owner/worker guards. `tests/capability-gate-reviews.test.ts` covers repeatable generation and gate vocabulary. Synthetic test decisions are never real hotel sign-off.
