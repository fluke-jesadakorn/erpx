# Native Field Service parts component

This contract describes the guarded native reserve-part and issue-part component under the accepted [Rust migration](../../adr/ADR-052-rust-backend-and-domain-aligned-frontend.md). The component began as an unmounted development-only library entry; the [owner closure](#owner-closure-2026-11-04) below records its later integration into the six merged Field Service workflows. The [production plan](../../production-plan.md) and [owner register](../../status/rust-backend-migration.json) retain the delivery gates; all 41 business-owner completion flags remain false until the owner register itself is updated.

## Entry and authority

[FieldServicePartsAdministration](../../../src/composition/src/field_service_parts/administration.rs) rejects production configuration. It accepts the normal host, device proof, Holding workspace request, Order, body and command key. The Account/SSO/IAM authorizer resolves current authority and prepares the owner sources within the Holding transaction. The request is forced to be a workspace mutation, its Company must match configuration, and the current Holding and Company are checked again. Current `records.read` and `records.write` are required before wire normalization. Workspace selection and its current-version checks are separate from the later Field Service Order compare-and-set; a supplied identifier or historical row is not an authorization witness.

[Service composition](../../../src/composition/src/field_service_parts/command.rs) binds typed public owner ports and a Workflow receipt within one retained transaction. Field Service owns Order and event intent; Support retains the linked case image; Inventory and Quality own stock, lot and hold facts; Finance owns journal and Money facts; Collaboration owns notification and Inbox effects. Commercial purchase and current owner authority remain independently checked. The [source preparation](../../../src/composition/src/field_service_parts.rs) locks owner sources before the authorizer's first Holding snapshot; those locks grant no rights or proofs. The retained Account/SSO/IAM context and [owner-public readers](../../../src/composition/src/field_service_parts/source.rs) supply and recheck actual authority and source proofs. Copied labels or caller-created structures do not grant those rights. The development entry does not create or start the visit.

The existing administrative entry retains read/write authorization before wire normalization. A separate internal [selected Identity ingress](rust-selected-ingress.md) accepts typed Order/key data and performs schema normalization after current Identity but before ordinary owner authorization; it is not an HTTP transport.

An additional [shared HTTP parts candidate](rust-field-service-http.md) now uses the actual Account request parser and selected transport ingress for reserve/issue. Its source-qualified PostgreSQL cases preserve the distinct wire, owner, replay and transaction failure channels. It remains unmounted; other POST workflows require an explicit dispatcher, and complete Field Service HTTP/SSR/frontend and runtime migration remain open.

## Reserve and issue

Reserve prepares the intended reservation and stock evidence, locks and rechecks availability, inserts the reservation, checks the resulting stock view and Support image, then writes the Order/event and notification. Issue rechecks the actual reservation and Inventory/Quality sources, Finance posting mapping and purchase evidence. It applies the reservation, persists the Finance journal, writes consumption and lot evidence, persists stock cost and stock, checks Support, then writes the Order/event and notification. Workflow owns the command receipt, audit and outbox. After deferred constraints, retained expected images and witnesses are checked again; an unexpected trigger rewrite cannot become a new accepted baseline. Failure rolls back the shared unit.

The separate Order CAS happens at its original late phase. Reserve and issue preserve the applicable stock and lot balance versions, including the observed V+1/V+2 behavior, rather than substituting the Order version. The prepared maximum-safe successor is provisional: earlier quote or stock failures retain their ordering, and only a successful current Order CAS can commit. Domain write clocks and current-authority guard clocks are distinct. Original phase ordering and error channels remain part of the contract; a passing synthetic result does not prove every possible wire value.

[Finance composition](../../../src/composition/src/field_service_parts/finance.rs) checks the current source and minimum factual lineage for its own journal/stock-cost effects. [Independent Finance review](../../../src/composition/src/finance_journal_review/field_service_parts.rs) uses Finance authority; a Field Service command is not a Finance approval. Raw stored UTF-8 historical references remain factual inputs subject to their owner checks, rather than newly invented producer authority or arbitrary current grants.

## Evidence and limits

At the frozen 109-entry source manifest beginning `1844f31`, the actual registered PostgreSQL targets passed 31 Field Service command cases, 16 Workflow command-ledger cases and 16 typed Inventory consumption-planner cases. The registered Inventory valuation target passed nine cases. These are bounded source-specific results, not owner acceptance. The earlier combined Field Service/Workflow epoch had 39 passes and eight failures (Field Service 24/7, Workflow 15/1); it is retained as correction evidence, not the final result. The additional Finance Maintenance and Issue Part regressions passed nine and five cases respectively through the common runner (25.102 and 21.133 seconds); these are bounded results, not whole-owner acceptance.

The [Field Service integration cases](../../../tests/integration/field_service/rust-parts-command-postgres.test.rs) exercise guarded administration, reserve/issue, replay, current authority, stock and lot behavior, source retention and deferred rollback. Some historical, start and maximum-version fixtures are directly seeded: they exercise factual compatibility and do not prove issuance by the original producer. The [Workflow cases](../../../tests/integration/workflow/rust-command-ledger-postgres.test.rs) independently cover the retained receipt and source boundaries. The bounded malformed UTF-16 lot identity edge is independently accepted for this guarded component. The current 11-path source manifest `80702185e9df929d793376a2e0ca8995388fb361b47d871d4d1040a7952aad09` matched before and after actual PostgreSQL runs: 33 Field Service, 16 typed Inventory planner, nine Finance Maintenance and five Maintenance Issue Part registered functions passed, with zero failed, ignored or filtered in 287.286 common-runner seconds. Ten valuation functions passed separately. The earlier 77-function evidence at `1844f31` overlaps these results and is not additive.

One native test function runs nine scenarios, not nine registered functions. A single malformed key commits using the replacement UTF-8 physical lot, replays exactly, rejects changed original input as an idempotency conflict, and permits independent Finance posting. Identical UTF-16 units or lexical equivalents reject early; distinct identities resolving to the same physical lot reach a late conflict after an observed first-slot effect and fully roll back. Observed lot metadata and other-balance tampering also causes source-change refusal and full rollback. The separate retained SQL leaf probe executes the original guard and observes CHECK `23514` after the first write; it is not an end-to-end retained B0 application transaction. The [12-case Node oracle](../../../scripts/testing/fixtures/generate-field-service-lot-identity-oracle.mjs) uses real Set behavior and driver bytes but fake Finance, SQL, CAS and effects. These results do not establish full wire or HTTP parity.

The subsequent codec-boundary correction removes the application dependency on Workflow JSON. Application carries opaque checked input and semantic details; the owner adapter reconstructs the original ordered values before subsequent SQL or clock reads. The original event JSONB encoding remains late. At matching pre/post source manifest `03c3f2714cf3c0860534527321e0f313df4cebd454bcf331afc56afb9745b390`, the actual Field Service33 and Workflow16 PostgreSQL targets pass (49 functions, zero failed/ignored/filtered, 211.958 common-runner seconds). Five policy functions and the unchanged six-case architecture guard also pass. These results overlap the preceding component evidence and do not establish hosted CI or runtime acceptance.

Run the registered lanes from the repository root with pinned Bun 1.4.3 and Node 26.11.1, using disposable configured PostgreSQL fixtures for integration:

```sh
bun run test:unit -- --suite=field_service.rust-parts-policy.unit
bun run test:unit -- --suite=inventory.rust-valuation.unit
bun run test:integration -- --suite=field_service.rust-parts-command-postgres.integration
bun run test:integration -- --suite=workflow.rust-command-ledger-postgres.integration
bun run test:integration -- --suite=inventory.rust-consumption-planning-postgres.integration
node scripts/testing/fixtures/generate-field-service-parts-oracle.mjs --check
node scripts/testing/fixtures/generate-field-service-parts-numeric-oracle.mjs --check
node scripts/testing/fixtures/generate-field-service-lot-identity-oracle.mjs --check
```

These commands and their registered inputs are in the [suite catalogue](../../../tests/catalogue/suite-manifest.json); oracle checks compare retained fixtures and do not establish database authority.

## Owner closure (2026-11-04)

The six original Field Service catalogue workflows and their required native dependencies were verified and merged through PR #17 (merge commit `2e0090832d85eb3c171672dd28729f8445d42802`). The [delivery register](../../status/rust-delivery-bundles.json) records `18/18` checklist items complete and `partial: false`, `verified: true`, `merged: true` for `field_service/service-orders`, `feature-agreements`, `feature-dispatch`, `feature-visits`, `feature-exceptions` and `feature-billing-release`.

The product path is native-only:

- `apps/service/.../field-service/route.ts` re-exports the shared handler; its POST dispatches every bundle command through `nativeReservePost` and throws `503 NATIVE_SERVICE_UNAVAILABLE` when the native capability is not configured. There is no legacy command fallback.
- `apps/service/.../field-service/{parts-setup,initial-physical-stock,customer-preparation,operations,collaboration}/...` re-export native-only handlers. The workspace/detail SSR presenters call the native workspace/detail readers. `packages/feature-service` and `packages/workplace` have no `@erpx/server-field_service` product import.
- Native HTTP routing covers workspace, detail, work, operations, collaboration, support source read, executive reads/evidence, parts setup/prerequisites/stock, customer preparation, initial physical Count and every workflow command/readback listed above.
- Native jobs cover the Field dependency/outbox projector, Files scan/cleanup, push delivery and initial Count projection. The private production supervisor restarts stopped workers; the Field Files scan worker accepted durable production databases in `files.rust-field-files-private-api.provider`.
- Retained `packages/server-field_service` TypeScript modules are read-only transport, projection, source-policy or partition adapters for named other-owner consumers; they are not a Field command writer. The shared modules that carry Field policy or owner SQL now state their consumer and removal condition. No schema, data, migration, audit or receipt history is removed.

Two external boundaries remain explicit:

- Finance production composition is mounted with the production environment and gated at runtime by an accepted Company production release (`FINANCE_PRODUCTION_RELEASE_REQUIRED`); parse-time rejection of Finance documents is no longer used. The Field billing release creates its draft invoice through the Finance owner Rust interface.
- Support/Collaboration/Operations/Records/Runtime/Workflow/Work/Identity/Finance/Customer consumers still use the retained read-only Field bridges listed above. Their removal condition belongs to those owners' migrations, not to deleting Field history.

Evidence: `docs/testing/evidence/2026-11-03-field-catalogue-scopes.json`, `field-writer-ownership.json`, `field-integrated-exact-candidate.json`, `field-production-runtime.json`, `field-production-transport-and-header.json`, `field-cold-hydration.json`, `field-modularity-withdrawal.json`, `field-records-source-guards.json` and `field-bundle-merge.json`.
