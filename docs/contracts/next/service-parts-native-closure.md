# Service Parts preparation: native writer closure

This bounded capability follows the merged Count and Field Files runtime deliveries. It closes the Service preparation surface: posting-map initialize/confirm-existing, four Finance prerequisites, Stock product/warehouse preparation and their three scoped reads. Existing native coordinators and bounded evidence are reused where their source and dependencies match. Full Finance/Commerce/Inventory owners, complete Field workflows and Records transactional consumers remain outside this capability.

## Admission checklist

- [x] Inventory exact API/internal/job/retry/read consumers and unchanged behavioral assertions; retain immutable sources/receipts and separate historical compatibility cases.
- [x] Port the live setup assertions to real Rust over the same leased database, preserving context/version, authority/purchase, replay/conflict/lost reply, deferred mutation/rollback and single receipt/audit/outbox checks.
- [x] Establish one authoritative native writer on every scoped entry; remove replaced TypeScript setup/prerequisite/master implementation and exports without removing generic non-Service behavior.
- [x] Service frontend uses native Stock/prerequisite reads with explicit unavailable503 and no fallback; scoped retained ProductEntry authority is documented.
- [x] Normal-auth Finance/Stock preparation through Count/post, confirm-existing and affected owner guards qualify; current-context failures and hydration findings are classified from actual runtime evidence.
- [x] Post-removal targeted PostgreSQL/HTTP/frontend, reference/quality checks and independent review pass; [PR #12](https://github.com/fluke-jesadakorn/erpx/pull/12) merged into the migration branch at `443623c863c09d5a8a89cd239a97b3a85784b8b9`. Docker/runtime qualification is separate.

This checklist is distinct from Field18, workflows206 and owners41. Nothing is complete merely because the current BFF is native-only or a legacy writer is no longer reached by one browser route.

## Source and retained consumers

The existing Rust composition is `src/composition/src/service_parts_posting_setup.rs`, `service_parts_prerequisite_setup.rs` and `service_stock_setup.rs`; the current native Next adapter is `packages/feature-field_service/src/adapters/server/native-parts-setup.ts`. The public parts-setup BFF/settings page already select these native ports and fail closed when absent. The replaced internal legacy write exports are removed.

Commerce retains only the actual `service-parts-posting-setup` ProductEntry authority reader. The duplicate Stock/master implementation and Finance prerequisite/rate wrappers are removed. Generic Finance/Commerce functions used outside this Service capability remain. The Service presenter now requires native Stock/prerequisite reads and fails503 if unavailable; the legacy fallback is removed.

`packages/server-identity_access/src/adapters/product-workspace.ts` and `packages/server-work_projection/src/adapters/product-launcher.ts` retain `servicePartsPostingSetupProductEntry` authority reads. Their genuine Finance/Stock assignment, current IAM and purchase semantics must remain until equivalent native entry behavior is migrated and verified; they do not authorize duplicate setup writers. Shared contracts/types are retained only for actual consumers, with no compatibility business writer.

## Evidence and next blockers

The [native HTTP contract](rust-field-service-http.md) records genuine mapping initialize/lost reply/replay, confirm-existing PostgreSQL/HTTP, four prerequisite writers and zero-reference Finance plus Stock-only preparation. The [Count delivery](rust-service-count-delivery.md) records same-database setup to Count and two-Company current-context behavior. Older bounded setup browser receipts reported Company/parts-entry hydration findings and no confirm-existing browser qualification. The separate confirm-existing normal-auth functional journey now passes with an unchanged historical mapping. The shared cold-entry hydration finding remains Field checklist16 and is not erased by either journey.

Live integration setup cases at `tests/integration/field_service/field-service.test.ts` now execute the real native leased fixture with their assertions retained. Previously declared historical compatibility Count cases remain historical. HTTP-only source reads cannot replace same-transaction Records guards in Support, Collaboration or Operations. That is a subsequent explicit Field blocker requiring its consuming native commands and transactional witnesses, not an addition hidden inside this setup bundle.

## Current transaction findings

Porting the original live assertions to the same native PostgreSQL lease exposed two real finalization gaps: deferred source changes ran after the last owner check, and receipt metadata could change during final assertion cleanup or deferred outbox effects. The native mapping coordinator now flushes deferred effects after ledger cleanup and revalidates the prepared owner witnesses and current authority/purchases. New-command receipt identity is captured during the ledger's final verification before cleanup; replay retains current-start owner/receipt witnesses, allowing lawful reference changes between commands while rejecting changes inside the replay unit.

The three live native cases pass at composition source SHA-256 `0d6f68957626eb2de736cc006f7476eac559ff0697dbd0c66e59e99babddc2d6`, including sequence-proven deferred Finance reference and Sales edition rollback, real replay-time mapping mutation, and new receipt cleanup/deferred mutation checks. The replay fault uses a scoped disposable PostgreSQL RLS read predicate because native replay performs no assertion deletion; it does not add a production hook or alter the original receipt. Original failed runs remain private evidence. The historical setup-prelude ports and post-removal seven-case run also pass. These individual case counts do not establish full Field or owner completion.

## Immediate source withdrawal

The post-removal registered PostgreSQL run passed all seven original live/historical cases on the native lease. The three former production command-schema modules now exist only as byte-exact test-support oracles, with original source paths and SHA-256 headers; the existing94-case UI/application suite passes with its assertions unchanged. Product code cannot import these test oracles.

Removed scope includes Commerce Service mapping/Stock workspace/master writers, the unused Service transaction bridge, Finance Service-only chart/account/period/rate wrappers and prerequisite read orchestration, their now-unreferenced source helpers, public exports and package exports. No stored data, tables, audit, migration history or receipts were deleted. The remaining Commerce `service-parts-posting-setup` public entry exports only the actual ProductEntry authority read. Identity product workspace and Work Projection launcher remain its consumers. Count's source-proof/obligation readers and generic POS/Sales/Finance methods remain until their separate migrations. Service's customer preparation is also a separate retained capability.

The product source inventory has been corrected to reference the native Parts coordinators/domains and retained authority readers. A pre-existing deleted Service Count domain reference was corrected to its Rust domain. Regeneration preserves exactly206 unique workflow IDs; source inventories do not establish workflow or owner acceptance.

## Actual browser boundary and remaining Field16 finding

The zero-reference case used normal Account/Holding SSO and distinct current actors: Finance preparation, FX approval, Stock preparation and independent Count review/posting. A committed Count response was dropped and retried with its original identity; acknowledged preparation operations were not replayed when changing actors or correcting harness assumptions. SQL finds one posted Count at version4, its source/line/ledger/journal, a balanced600/600 two-line journal and distinct maker/reviewer. All six aggregate events are processed with six HoldingRead receipts and a checkpoint. There is no matching Work item and none is claimed. A fresh authenticated reload displays posted status; an earlier screenshot showed approved before refresh and is not posted-readback evidence.

The second case created its historical mapping through the actual Supply/Finance owners and normal Supply withdrawal using `tests/support/field_service/historical-supply-parts.ts`. Service confirmation preserved that mapping byte-for-byte, recorded `confirm-existing` and left Supply inactive. This is functional normal-auth evidence, not a clean cold-entry claim: the confirm journey retains hydration/premount and development-resource diagnostics. Warm resumed Count action/readback observations have no page/console errors, a narrower fact.

Cold read-only reproductions identify a Next development `serverComponentChanges` WebSocket frame initiating `hmrRefreshReducer`/`hmrRefresh` and an RSC request before the React streaming mismatch. Focus-handler and differing-provider-state fixes are not justified by that evidence. Field16 remains open; an isolated correctly configured release-mode artifact builds and serves namespace-qualified assets, but its readiness guard denies selecting the native leased Company. The engineering attestation expects `erpx_browser_<run>_*` identities while the native API requires `erpx_dev_rust_<token>` leases. No existing verified bridge was found; selected-Company release SSR remains unqualified and neither guard is relaxed. No SSR, streaming or hydration assertion is disabled. The first client diagnostic/DOM capture was overwritten by a harness attempt; the original server diff is preserved, later attempts have separate outputs, and missing original client metadata is explicitly not reconstructed as evidence.

See the [source-bound receipt](../../testing/evidence/2026-10-31-service-parts-native-closure.json). [Docker qualification](service-parts-containers.md) follows as a separate scoped delivery using the shared modular-monolith image; the container-harness delta was excluded from the Rust PR. Complete workflow/owner counts remain unchanged.
