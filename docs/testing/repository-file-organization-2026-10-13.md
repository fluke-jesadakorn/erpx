# Repository file organization — 2026-10-13

This report records file placement and verification for the reorganization begun on 2026-10-12. It is not a delivery plan or a provider/Company release acceptance claim.

The [relocation register](repository-file-relocations.json) records 1,134 file moves and 450 removed legacy forwarders, including original checksums. All 517 existing suite definitions retain their IDs, owners, lanes, prerequisites and cases at their mapped paths. A new runtime packaging suite brings the catalogue to 518 suites. The sole compatibility entry, `src/server/runtime/env.ts`, remains for the unchanged staging bootstrap.

## Placement

Executable tests use `tests/<lane>/<owner>/`; helpers/setup use `tests/support/`; runner inputs use `tests/catalogue/`; runner/typecheck configuration uses `tests/config/`. Executable scripts are grouped by testing, performance, seed, docs, assets, UI and local runtime operations. Fixture factories/data use owner folders, while executable fixture preparation uses `scripts/testing/fixtures/`. Components and lib helpers use owner folders and explicit shared categories. Server implementations remain inside the existing domain/application/adapters/public architecture.

`bun run structure:check` enforces placement alongside catalogue and architecture checks. Package command names remain unchanged. Reports and screenshots retain their ignored root output directories even though Playwright configuration moved.

## Verified

- Unit: 201 files / 2,800 cases passed on the final code, including standalone-link relocation tests.
- Integration: the complete 225-file selection passed 223 files / 2,099 cases initially. Two mock import paths were corrected and both affected files passed on rerun (11 additional cases), covering all 225 files / 2,110 cases. This is combined coverage, not a claim that the original full invocation exited successfully.
- Browser: 14 explicitly selected desktop/mobile cases across seven E2E, journey and UAT suites passed in forward and reverse order. A declared harness failure proved owned-cohort cleanup before reverse execution. Login/session persistence, People settings, dirty drawer keyboard behavior, Finance invoice drawers, request recovery, Legal review and invalid-ID recovery were covered.
- Additional broad-run proof: PMS booking/payment/refund recovery passed on desktop and mobile. The broad browser run was interrupted with no cleanup failures; all 2,701 registered browser requirements were not executed twice.
- Catalogue, architecture/terminology/placement, documentation, app/test typecheck and isolated standalone build passed. Lint has zero errors and three pre-existing warnings.
- Body comparison verified 1,038 page/component implementations unchanged after excluding module-path relocation. Checksums of 555 protected files, including immutable evidence and staging-specific files, remain unchanged.
- UI standard guard retains the same 130 pre-existing findings as the freshly generated pre-relocation baseline, with zero new findings. No UI redesign or appearance fixes were made.

## Runtime and packaging

Browser execution exposed dangling standalone dependency links that referenced the deleted build tree. Release packaging now rewrites internal traced links relative to the release and rejects external links. Four dependency aliases resolve inside the standalone release after its build workspace is removed. Tests verify relocation, canonical package identity and refusal of external dependencies.

The missing development database manifests were recreated with explicit user authorization. Only the passwords of 24 existing restricted `erpx_dev_*` app/jobs/migration roles changed. No databases, schemas, business data or application login credentials were created or altered. Private manifests use mode 0600, and the development jobs worker remains detached. Production and staging configuration were not changed; no deployment was performed.

Verification artifacts, original snapshots and native/browser logs remain in ignored `.scratch/reorganization/` and `.scratch/dev/test-suites/browser/`. Current runtime checks use the development console and private production health endpoint, not staging.

## Required-case follow-up

The user selected one execution of all 2,701 required browser cases instead of the full forward/reverse run. Concurrent IAM/People edits changed the source fingerprint and stopped both runners. At the user's request, the round ended with 1,430 passed, 70 failed and 1,201 unverified requirements. The [extended verification report](repository-required-browser-verification-2026-10-13.md) records the tested candidate and every case's status; these results do not certify the subsequently changed working tree.

The preferred-role register had two desktop/mobile labels that differed from the existing test file before and after relocation. With explicit user approval, only those two register titles changed from `searchable selector has no Automatic option and appearance stays compact` to `role switcher has no input or Automatic option and appearance stays compact`. Assertions, exclusions and case counts remain unchanged. All six preferred-role requirements passed against the updated register.

Retained results from before this approved label correction are admitted only after verifying unchanged runtime bytes and checksums of the other 571 test/CI files. The old and new verification fingerprints are recorded separately in ignored `.scratch/reorganization/preferred-register-admission.json`; original receipts are not rewritten. Renamed identities require fresh execution. An interrupted receipt with incomplete cleanup is excluded; its owned partial cohort was removed and its cases are selected again.

Extended verification also found that the UAT route generator still wrote its relocated JSON into the documentation directory. The generator now writes the runner's canonical catalogue file, and documentation checks reject stale route inventory or a duplicate documentation JSON. The catalogue and Markdown were regenerated together; existing immutable evidence remains unchanged.
