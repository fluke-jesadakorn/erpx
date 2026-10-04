# Verification and recorded evidence

The [production plan](../production-plan.md) defines the full-catalogue engineering scope. [Engineering scenarios](engineering-scenarios.json) map exact capability IDs to workflows and required integration/desktop/mobile cases; [gate scenarios](engineering-gate-scenarios.json), [Legal UAT](legal-uat-engineering-cases.json) and [attachment owners](engineering-attachment-owner-cases.json) supply the additional required identities. Preserve these registers: runners and source fingerprints read them directly.

The [2026-10-03 receipt](evidence/2026-10-03-engineering-completion.json) retains 187 capability and 12 gate results, the assessed date, environment, original source/verification fingerprints, compiled candidate identity and digests of its original result files. Its sanitized repository summary contains no credentials or session data. Raw logs, browser traces and the presentation package remain private under ignored `.scratch/dev/engineering-completion/`.

The [capability report](../status/capability-delivery-matrix.md) projects this dated receipt separately from provider evidence and Company acceptance. It is not a rerun against each subsequent source revision. Generator validation rejects missing/duplicate IDs, inconsistent totals and mismatched scenario-register bytes. A fresh engineering claim requires a fresh run, never a changed timestamp on an old receipt.

## Run and inspect

Use pinned Node 24.20.0 and Bun 1.2.17. `bun run test -- <test-files>` runs selected isolated native integration tests. `bun run completion:inventory` inspects the scenario register; the completion integration/verification commands perform the actual engineering work. See [E2E cases](e2e-specification.md), [isolated browser setup](isolated-browser-simulation.md), [working demo](working-demo-company.md) and [source-route inventory](uat-route-inventory.md) for their separate boundaries. Reuse development on 3100; isolate fixtures and avoid reserved 3000/3001/8080. Builds use `bun run deploy:build` rather than dev's output directory.

## Evidence retention

Files in `evidence/` and the [checksum ledger](../status/business-acceptance-evidence.json) are retained at their original paths/bytes. Original observations and supersession chains remain intact. The [explicit review source](../status/capability-gate-reviews.json) alone governs inventory gate attestations; generating or observing evidence never grants Company release approval.

Other reports in this directory retain their recorded findings. Their implementation gaps, counts, provider limitations and hostname observations describe the original run; use the current owner contracts and production plan for present behavior. Official-source research keeps its original effective/verification dates and limitations; no documentation cleanup renews legal or statutory certification.
