# Verification and recorded evidence

The [production plan](../production-plan.md) owns current migration scope and deferred full acceptance. [Owner contracts](../contracts/next/README.md) define behavior. Daily development and exact-candidate CI produce verification with `acceptance:false`; they do not certify a provider, Company release or installed deployment.

## Run and inspect

Use pinned Node 26.11.1 and Bun 1.4.3. `bun run test -- <test-files>` runs the selected native lane according to the strict catalogue. `bun run completion:inventory` inspects the scenario register; the completion integration/verification commands perform the actual engineering work. See [E2E cases](e2e-specification.md), [isolated browser setup](isolated-browser-simulation.md), [working demo](working-demo-company.md) and [source-route inventory](uat-route-inventory.md) for their separate boundaries. Reuse development on 3100; isolate fixtures and avoid reserved 3000/3001/8080. Builds use `bun run deploy:build` rather than dev's output directory.

Unit forks are capped at `min(8, os.availableParallelism())`, so child CLI validators and receipt checks share the actual CPU budget of a development machine or hosted runner. Each case retains its 30-second deadline and zero retries. Integration retains its separate two-worker limit and the explicit one-worker legacy benchmark mode; database admission and fixture leases remain independently enforced.

The current scoped runner reads the app/package graph and strict suite catalogue, including staged, unstaged and untracked changes relative to HEAD. Missing ownership/dependency mappings fail selection. Inspect before executing when the intended scope is uncertain:

```bash
bun run test:dev --list --json
bun run test:dev --app=finance --list
bun run test:dev --app=finance
bun run test:dev --watch
bun run test:ci --base=<base-sha> --head=<candidate-sha> --list
bun run test:ci --base=<base-sha> --head=<candidate-sha>
bun run build:affected
bun run test:unit -- --suite=<registered-suite>
bun run test:integration -- --suite=<registered-suite>
bun run test:e2e -- --module=people_payroll --list
```

Executing CI requires a clean checkout of its exact head commit. Scoped profiles select related native suites and registered desktop browser smoke, check structure/catalogue and affected tooling, and reuse only complete-input lint/typecheck/unit/build caches. PostgreSQL integration and browser cases execute against disposable fixtures. New edits cancel obsolete watch rounds. [E2E specification](e2e-specification.md) records required identities and browser limits.

The direct `test`/`test:unit`/`test:integration` commands retain explicit catalogue selection; mixed native files keep their own unit/integration lanes. Unknown selectors, wrong lanes and excluded explicit files fail. Unit tests have no PostgreSQL setup. Every integration/browser suite declares database, scanner/jobs and cleanup prerequisites. [Isolated browser setup](isolated-browser-simulation.md), [working demo](working-demo-company.md) and [route inventory](uat-route-inventory.md) define their separate scopes. Provider checks require their own explicit lane and credentials; an internal adapter run is not provider acceptance.

Reuse the owned gateway on3100, preserve reserved3000/3001/8080 and build only in isolated output directories. Use exact [Holding development hosts](../../ops/macos/holding-development.md) and normal authentication for public checks. Existing legacy loopback/product-host fixture modes and their receipts do not prove the current Holding topology.

## Full acceptance is deferred

`bun run release:acceptance --candidate=<clean-sha>` is reserved for an intentional production-preparation request. It requires the exact clean frozen candidate and a verified combined workspace-v2 app/gateway/jobs release; the reviewed workspace-v2 acceptance adapter remains a blocker. Do not run it during daily migration work or from normal CI, merge hooks or schedules.

Legacy `test:full`, `completion:integration` and `completion:verify` runners retain their original single-artifact/full-catalogue engineering scope. They are not daily verification commands or substitutes for workspace-v2 acceptance. Historical records retain the limits of the runner that actually produced them. The strict [engineering scenarios](../../tests/catalogue/engineering-scenarios.json), [gate scenarios](../../tests/catalogue/engineering-gate-scenarios.json), [Legal UAT](../../tests/catalogue/legal-uat-engineering-cases.json) and [attachment owner cases](../../tests/catalogue/engineering-attachment-owner-cases.json) remain executable inputs; do not delete them as old plans.

The [2026-10-03 engineering receipt](evidence/2026-10-03-engineering-completion.json) records187 capabilities and12 gates on its original source/verification fingerprints and compiled candidate. The [capability report](../status/capability-delivery-matrix.md) projects that dated receipt separately from provider and Company acceptance. A later generator run does not rerun or restamp it. Full required-case acceptance cannot substitute missing/skipped/retried cases or incomplete cleanup with a passing subset.

## Evidence retention

Files in `evidence/` and the [checksum ledger](../status/business-acceptance-evidence.json) are retained at their original paths/bytes. Original observations and supersession chains remain intact. The [explicit review source](../status/capability-gate-reviews.json) alone governs inventory gate attestations; generating or observing evidence never grants Company release approval.

Other reports in this directory retain their recorded findings. Their implementation gaps, counts, provider limitations and hostname observations describe the original run; use the current owner contracts and production plan for present behavior. Official-source research keeps its original effective/verification dates and limitations; no documentation cleanup renews legal or statutory certification.

## Owner isolation and selection

The [service boundary contract](../contracts/next/service-boundaries.md) defines typed ports and shared transactions. The [owner registry](../../packages/contracts/src/ownership.json) maps source ownership; the [suite manifest](../../tests/catalogue/suite-manifest.json) maps each executable suite to lane, owner, modules, apps/packages, fixtures and prerequisites. Do not hardcode suite totals here: `bun run test:catalogue` checks the current register, and scoped `--list --json` reports actual selection.

Use `bun run docs:check`, `bun run products:check`, `bun run structure:check` and `bun run architecture:check` for repository integrity. Documentation checks cover root Markdown, active links/anchors and archive navigation, preserve immutable historical link topology, validate the evidence ledger and reject existing tracked temporary files. New executable tests belong under their declared `tests/<lane>/<owner>/` and must be registered. Never widen authority, relax source checks or modify frozen evidence to reduce verification scope.
