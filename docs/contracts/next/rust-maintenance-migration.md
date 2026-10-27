# Native Maintenance Work component

This component follows [ADR-052](../../adr/ADR-052-rust-backend-and-domain-aligned-frontend.md). Maintenance remains incomplete in the [owner register](../../status/rust-backend-migration.json); the existing TypeScript runtime remains active. The [delivery plan](../../production-plan.md) owns sequencing and merge gates.

## Portable action policy

The original application checks the exact logged-in owner plus an eligible current worker, or both selected `records.review` and an independently eligible scoped reviewer. That refusal precedes the current-state refusal. The Rust domain preserves the seven actions and original state projections, including JavaScript epoch-zero truthiness for completion/cancellation downtime. The retained B0 fixture compares112 action decisions and21 transitions. The portable change use case also matches the nine separate schema/replay/ordering observations, through explicitly synthetic ports; this is not actual transactional command/effect evidence.

The pure facts and projections do not confer permission, purchase admission, stock mutation, journal posting or command replay authority. Frontend previews do not replace these backend checks.

## Input and application boundary

The bounded Rust wire adapters compare54 actual B0 schema/Fetch cases with pinned Node26.11.1 and the same Zod4.6.7 recorded in B0's lockfile. The fixture records exact dependency-source hashes. It covers every action, creation, unknown-field stripping, duplicate-last fields, defaults/nulls, numeric/date failures, array-bound-before-child failure order, exact issue order and normalized property order. String limits follow that pinned Zod's Unicode code-point behavior. ECMAScript trim preserves non-trim whitespace such as U+0085. Lone UTF-16 units remain distinct in normalized receipt data even when PostgreSQL UTF-8 storage replaces them.

Sealed normalized inputs belong to the application boundary as immutable canonical bytes; adapters construct them and reparse those exact bytes at the Workflow boundary. Application imports neither adapters nor the mixed Workflow façade. The change use case authorizes before invoking deferred normalization or constructing its repository, checks exact replay before reading Work, applies owner refusal before state refusal, then reads the clock and prepares a typed consumption value before the repository's final version/effect step. That prepared value belongs to this one command. Actual selected context, purchase scope, original Workflow replay and atomic owner/Inventory effects must still be supplied by the native composition; synthetic ports confer none of those permissions.

## Current owner sources

People supplies only a linked active/unmerged Company Person and an active engagement effective on the observed UTC date, including inclusive start/end dates. Identity combines that reference with an actual active principal/assignment, published `records.write` or `records.review`, and the Work's actual Company/Branch/Department. The malformed Holding scope rejection follows [ADR-055](../../adr/ADR-055-native-directory-scope-and-credential-witness.md). These reference checks do not require or infer a People purchase.

Assets owns a minimal current physical Asset reference: identifiers, Company/Branch/Department and version. It transfers no name, cost, currency, custodian or private profile. Presence is not lifecycle admission. The opaque reference retains database, backend, full transaction ID and server-start witnesses; a changed version, another transaction or an older conflicting snapshot fails.

Maintenance's bounded Work reader borrows the original sealed `AuthorizedWorkContext`, requires selected `records.write` plus the Maintenance module, and obtains Company/scope from that context. It reads only the actual Work identifiers, owner, state, version and relevant timestamps. Submitted scope labels cannot substitute for that source. The linked physical Asset must have the same Company/Branch/Department.

The source preparation acquires utility fences before IAM establishes the first Holding snapshot, including the original Maintenance insertion/absence fence. It obtains the Asset before locking the Work tuple and retains current owner/reviewer references. Final reference checks reject local source changes, backwards time, expired context and another transaction. The expected-version assertion is separate so eventual Inventory preparation can preserve the original failure order.

## Remaining command boundary

This reference is not an issue-part command or a stock write capability. The actual owner command still needs the exact original Workflow replay/input fingerprint, Assets capability admission (`settle-existing`, or History for an exact prior receipt), Inventory reservations/date/Quality checks, Finance and Money effects, and exact final source/event/stock/lot/journal after-images in one transaction. Full purchase source images must not be replaced by this minimal reference. Notifications/outbox and source-version advancement remain part of that same unit.

Start/complete/cancel also require Assets' actual lifecycle/version/evidence effects. Preventive plans, actual creation/scheduling commands, queries/SSR, owner HTTP, jobs, notifications and browser journeys remain pending. No native Maintenance route is mounted and no business-owner completion is claimed.

## Component verification

The registered targets are `maintenance.rust-work-policy.unit`, `maintenance.rust-work-wire.unit`, `maintenance.rust-work-commands.unit`, `assets.rust-maintenance-source-postgres.integration`, and `maintenance.rust-work-source-postgres.integration`, and `maintenance.rust-issue-part-command-postgres.integration`; People and Identity retain their separate current-reference targets. The Asset cases use disposable migrated PostgreSQL and cover minimal scope, local changes/transaction escape, actual tuple retention and old-snapshot40001. The Work target uses real retained Account/Holding authorization, actual worker/Asset rows, independent review versus exact owner, selected scope, expiry and final revalidation. These are component checks, not full migration acceptance.


## Retained issue-part Work and event effects

A native effect adapter borrows the original current Work reference and sealed normalized command. It derives the intended next Work/event images before effects; expected-version comparison remains the late apply gate after Inventory/Finance preparation. It creates no commercial purchase, inventory permission or independent command receipt. Identity/People/Asset witnesses remain current through the enclosing Account/Holding authorization unit.

Complete owned Work and event rows are hashed inside PostgreSQL before writing, including unchanged title/evidence fields without transferring large private bodies. The adapter writes only the original issue-part transition and exact actor/version event. Final retention validates those predetermined full-row images and the original external witnesses after deferred constraints. It cannot rebaseline the old Work reference after an intentional version increment.

The expanded8-case Work-source target passes on real retained Account/Holding authorization and disposable migrated PostgreSQL. New cases verify the native Work/event effects and atomic rollback when immediate or deferred triggers rewrite an unrelated title. Inventory/Finance/purchase/Workflow/notification assembly remains required before this is an executable owner command; no native route is activated.


## Native parts command assembly (unmounted)

The Rust service composition in `src/composition/src/maintenance_parts.rs` and its private source module assembles the sealed normalized command and original Account/Holding context through Workflow, Commercial, Inventory, Finance and Collaboration. Maintenance owns only its Work policy, SQL, facts and owned plan/receipt; immutable holding/user/assignment and command/time getters provide data, never a new Context or authority factory. Its source preparation obtains the fixed owner utility fences before the first Holding snapshot. There is no independent commit, second Finance command receipt, `finance.write` requirement, source/actor DTO grant or TypeScript calculation call. Runtime package mode comes from trusted composition rather than command JSON.

Readonly preparation retains the original Work/worker/Asset proofs and Inventory sources, calculates Journal/Money, and derives predetermined Work/event and notification/Inbox images before Workflow inserts its known-result receipt. Owned preparation carriers span Workflow's prepare/apply phases without self-referential structures or copying an opaque authority. Actual persistence must reborrow the same sealed Work source and exact original bindings. Finance remains a Draft; posting/review and Maintenance complete/cancel settlement gates are separate work.

The write sequence is the original late worker/version and unreserved-stock/date guards, Finance journal, consumption and lot evidence/moves, Money, stock, Work/event, staff notification and the existing Inbox trigger. Workflow then appends audit/outbox. After deferred constraints, the adapter checks the predetermined complete owned images and original factual/authorization witnesses again. A late outbox trigger must not turn a changed Work into an accepted new baseline.

Domain write clocks and current-guard clocks are distinct dependencies. Domain observations remain Work, consumption after unique-period selection and before lot reads, journal, Money, and Workflow receipt/audit/outbox. The real native Finance/Inventory/Commercial component case has verified those first four observations and Draft effects without extra Finance permission, plus detached Finance/Work receipt retention, journal tampering and cross-transaction refusal. The separately registered full Workflow command target passes3 actual PostgreSQL cases: one receipt with exact replay and all notification/Inbox effects, original stock/quote/late-version refusal for maximum-safe input, and complete rollback after a deferred outbox-trigger Work rewrite. The Work-source target passes8 cases after the owned preparation/receipt refactor. The unchanged eight/three targets have also passed after moving the complete foreign-port binding to `erpx-service`; their paths, suite IDs, cases and assertions remain unchanged. These remain component evidence for an unmounted command.

A valid maximum-safe requested version may carry its provisional successor through readonly preparation. Stock/quote errors retain their original precedence; the actual late Work CAS rejects a stale request. The conditional Workflow result uses the actual current successor, so a stale maximum-safe request cannot accidentally turn into an earlier generic output-range error. Every committed success still requires requested and actual versions to match. The shared Workflow bound continues to reject an actual persisted source whose successor is outside safe output units; no complete unsafe-source parity is claimed.

No route or installed runtime is activated. Creation/scheduling, other transitions, parts-posting review, preventive plans, queries/SSR, jobs and browser journeys remain migration gates, and all business-owner completion flags remain false.
