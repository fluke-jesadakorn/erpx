# Native Field Service parts component

This contract describes the guarded native reserve-part and issue-part component under the accepted [Rust migration](../../adr/ADR-052-rust-backend-and-domain-aligned-frontend.md). It is an unmounted development-only library entry, not a completed Field Service owner or an installed runtime. The [production plan](../../production-plan.md) and [owner register](../../status/rust-backend-migration.json) retain the delivery gates; all 41 business-owner completion flags remain false.

## Entry and authority

[FieldServicePartsAdministration](../../../src/composition/src/field_service_parts/administration.rs) rejects production configuration. It accepts the normal host, device proof, Holding workspace request, Order, body and command key. The Account/SSO/IAM authorizer resolves current authority and prepares the owner sources within the Holding transaction. The request is forced to be a workspace mutation, its Company must match configuration, and the current Holding and Company are checked again. Current `records.read` and `records.write` are required before wire normalization. Workspace selection and its current-version checks are separate from the later Field Service Order compare-and-set; a supplied identifier or historical row is not an authorization witness.

[Service composition](../../../src/composition/src/field_service_parts/command.rs) binds typed public owner ports and a Workflow receipt within one retained transaction. Field Service owns Order and event intent; Support retains the linked case image; Inventory and Quality own stock, lot and hold facts; Finance owns journal and Money facts; Collaboration owns notification and Inbox effects. Commercial purchase and current owner authority remain independently checked. The [source preparation](../../../src/composition/src/field_service_parts.rs) locks owner sources before the authorizer's first Holding snapshot; those locks grant no rights or proofs. The retained Account/SSO/IAM context and [owner-public readers](../../../src/composition/src/field_service_parts/source.rs) supply and recheck actual authority and source proofs. Copied labels or caller-created structures do not grant those rights. The development entry does not create or start the visit.

## Reserve and issue

Reserve prepares the intended reservation and stock evidence, locks and rechecks availability, inserts the reservation, checks the resulting stock view and Support image, then writes the Order/event and notification. Issue rechecks the actual reservation and Inventory/Quality sources, Finance posting mapping and purchase evidence. It applies the reservation, persists the Finance journal, writes consumption and lot evidence, persists stock cost and stock, checks Support, then writes the Order/event and notification. Workflow owns the command receipt, audit and outbox. After deferred constraints, retained expected images and witnesses are checked again; an unexpected trigger rewrite cannot become a new accepted baseline. Failure rolls back the shared unit.

The separate Order CAS happens at its original late phase. Reserve and issue preserve the applicable stock and lot balance versions, including the observed V+1/V+2 behavior, rather than substituting the Order version. The prepared maximum-safe successor is provisional: earlier quote or stock failures retain their ordering, and only a successful current Order CAS can commit. Domain write clocks and current-authority guard clocks are distinct. Original phase ordering and error channels remain part of the contract; a passing synthetic result does not prove every possible wire value.

[Finance composition](../../../src/composition/src/field_service_parts/finance.rs) checks the current source and minimum factual lineage for its own journal/stock-cost effects. [Independent Finance review](../../../src/composition/src/finance_journal_review/field_service_parts.rs) uses Finance authority; a Field Service command is not a Finance approval. Raw stored UTF-8 historical references remain factual inputs subject to their owner checks, rather than newly invented producer authority or arbitrary current grants.

## Evidence and limits

At the frozen 109-entry source manifest beginning `1844f31`, the actual registered PostgreSQL targets passed 31 Field Service command cases, 16 Workflow command-ledger cases and 16 typed Inventory consumption-planner cases. The registered Inventory valuation target passed nine cases. These are bounded source-specific results, not owner acceptance. The earlier combined Field Service/Workflow epoch had 39 passes and eight failures (Field Service 24/7, Workflow 15/1); it is retained as correction evidence, not the final result. The additional Finance Maintenance and Issue Part regressions passed nine and five cases respectively through the common runner (25.102 and 21.133 seconds); these are bounded results, not whole-owner acceptance.

The [Field Service integration cases](../../../tests/integration/field_service/rust-parts-command-postgres.test.rs) exercise guarded administration, reserve/issue, replay, current authority, stock and lot behavior, source retention and deferred rollback. Some historical, start and maximum-version fixtures are directly seeded: they exercise factual compatibility and do not prove issuance by the original producer. The [Workflow cases](../../../tests/integration/workflow/rust-command-ledger-postgres.test.rs) independently cover the retained receipt and source boundaries. Distinct malformed UTF-16 lot identifiers that collapse to the same UTF-8 replacement remain **unaccepted**; full wire parity is not established.

Run the registered lanes from the repository root with pinned Bun 1.4.3 and Node 26.11.1, using disposable configured PostgreSQL fixtures for integration:

```sh
bun run test:unit -- --suite=field_service.rust-parts-policy.unit
bun run test:unit -- --suite=inventory.rust-valuation.unit
bun run test:integration -- --suite=field_service.rust-parts-command-postgres.integration
bun run test:integration -- --suite=workflow.rust-command-ledger-postgres.integration
bun run test:integration -- --suite=inventory.rust-consumption-planning-postgres.integration
node scripts/testing/fixtures/generate-field-service-parts-oracle.mjs --check
node scripts/testing/fixtures/generate-field-service-parts-numeric-oracle.mjs --check
```

These commands and their registered inputs are in the [suite catalogue](../../../tests/catalogue/suite-manifest.json); oracle checks compare retained fixtures and do not establish database authority. Complete owner workflows, HTTP/SSR and frontend integration, native start workflow, jobs, providers, authenticated browser journeys and runtime activation remain open. PR1 requires the full native migration and unchanged acceptance gates before the separate PR2 container preparation; this component does not advance those gates by itself.
