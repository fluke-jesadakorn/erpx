# Development attachment reconciliation closeout — 2026-09-28

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

The user explicitly approved retirement of the 133 synthetic local development reconciliation blockers. This approval applied only to `erpx_dev_*` databases and the local development file namespace. Staging and private production were not changed. The global attachment feature gate remains closed; this is data readiness, not browser or deployment acceptance.

## Result

The frozen `--apply --scan --checksum --final` run in `.scratch/dev/final-attachment-reconciliation-2026-09-28.json` reports `gateReady: true`, **zero adjudication items**, and **zero blocking items**. H001, H002, H003, and Billing each have a ready final receipt. All Holding unknown ownership/purpose, unavailable sources, missing ledger, orphan object, checksum/object, cross-Company, duplicate-ledger, and quota-delta findings are zero. Billing has zero orphan objects and zero quota delta.

Before remediation, a fresh read-only audit reproduced all 133 items: 107 unavailable Holding sources (36 unknown-purpose/scope and 71 malformed scan-error fixtures), 23 Billing orphan objects, two quota deltas, and the required final-reconciliation action. Category counts overlap for the same source. The exact dry-run matched the known six source groups and rejected other blockers or Legal Holds.

## Controlled development-fixture reset

The operator used [the local-only retirement script](../../scripts/local/retire-attachment-fixture-blockers.ts) after confirming the feature gate was false and both web and jobs reported upload freeze. The script required the exact expected counts, `erpx_dev_*` database names, the local-test data root, non-beta slots, known fixture owner/state groups, verified target Company for unknown sources, clean object checksums, no Legal Hold, no derivative/publication/preservation references, and no unexpected reconciliation defect.

Run ID: `attachment-fixture-retirement-2026-09-28T03-01-33-918Z` (UTC). A private archive at `.scratch/dev/attachment-fixture-retirement-2026-09-28T03-01-33-918Z/` contains the operator decision and source-report digest, four PostgreSQL custom-format dumps, a manifest, and 130 original object-byte backups. All 130 archived objects passed a post-write SHA-256 and length check. The archive is ignored by Git and its directory and backup files are private to the local machine.

For this approved synthetic-data reset only, the script temporarily disabled the two source-state protection triggers inside each Holding transaction, changed the 107 fixture source rows to `destroyed`, assigned the 36 unscoped rows their independently verified target Company, transitioned existing storage-ledger rows to `destroyed`, appended 107 `dev.synthetic_fixture_retired` events, and re-enabled the triggers before commit. It retained every source, bound link, scan attempt, ledger, and prior audit row. It then removed exactly the 107 retired source objects and 23 Billing orphan objects from the active local/MinIO file namespace. The bytes remain in the private recovery archive. This is **not** the generic retention or security-destruction mechanism and is not an approved procedure for real evidence or other environments.

Post-operation checks found 92 retirement events/destroyed sources in H001, 15 in H002, and none in H003. The final reconciliation then verified zero unresolved findings without manually inserting or editing readiness receipts. The temporary upload freeze was removed; local dev again reports `enabled: false`, `uploadFrozen: false`, scanner/preview/jobs/migration/Billing ready, with web and jobs running under the pinned development toolchain.

## Verification and next release gates

- `bun run typecheck` passed.
- Reconciliation and operations-health tests passed: 2 files, 10 tests, using the repository's Vitest runner (`bun run test -- ...`). A direct `bun test` invocation was not valid for this repository and was superseded by the passing run.
- The development gate is intentionally still false. Authenticated desktop/mobile browser acceptance and public-media/sensitive-document checks remain outstanding; a clean reconciliation alone does not authorize activation.
- Staging and production still run the older release. Their migrations, scanner readiness, independent backfill/final receipts, and browser acceptance remain separate operator work.
- All five required health endpoints returned HTTP 200 after the dev freeze was removed: local/public dev, local/public staging, and private production. The affected Leave path through both public hostnames redirected to the respective console hostname and reached HTTP 200 at the unauthenticated sign-in destination; authenticated UI behavior was not verified by these HTTP checks.
