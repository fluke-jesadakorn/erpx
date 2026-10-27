# Native Maintenance Work component

This component follows [ADR-052](../../adr/ADR-052-rust-backend-and-domain-aligned-frontend.md). Maintenance remains incomplete in the [owner register](../../status/rust-backend-migration.json); the existing TypeScript runtime remains active. The [delivery plan](../../production-plan.md) owns sequencing and merge gates.

## Portable action policy

The original application checks the exact logged-in owner plus an eligible current worker, or both selected `records.review` and an independently eligible scoped reviewer. That refusal precedes the current-state refusal. The Rust domain preserves the seven actions and original state projections, including JavaScript epoch-zero truthiness for completion/cancellation downtime. The retained B0 fixture compares112 action decisions and21 transitions; nine separate schema/replay/ordering cases remain pending native command composition.

The pure facts and projections do not confer permission, purchase admission, stock mutation, journal posting or command replay authority. Frontend previews do not replace these backend checks.

## Current owner sources

People supplies only a linked active/unmerged Company Person and an active engagement effective on the observed UTC date, including inclusive start/end dates. Identity combines that reference with an actual active principal/assignment, published `records.write` or `records.review`, and the Work's actual Company/Branch/Department. The malformed Holding scope rejection follows [ADR-055](../../adr/ADR-055-native-directory-scope-and-credential-witness.md). These reference checks do not require or infer a People purchase.

Assets owns a minimal current physical Asset reference: identifiers, Company/Branch/Department and version. It transfers no name, cost, currency, custodian or private profile. Presence is not lifecycle admission. The opaque reference retains database, backend, full transaction ID and server-start witnesses; a changed version, another transaction or an older conflicting snapshot fails.

Maintenance's bounded Work reader borrows the original sealed `AuthorizedWorkContext`, requires selected `records.write` plus the Maintenance module, and obtains Company/scope from that context. It reads only the actual Work identifiers, owner, state, version and relevant timestamps. Submitted scope labels cannot substitute for that source. The linked physical Asset must have the same Company/Branch/Department.

The source preparation acquires utility fences before IAM establishes the first Holding snapshot, including the original Maintenance insertion/absence fence. It obtains the Asset before locking the Work tuple and retains current owner/reviewer references. Final reference checks reject local source changes, backwards time, expired context and another transaction. The expected-version assertion is separate so eventual Inventory preparation can preserve the original failure order.

## Remaining command boundary

This reference is not an issue-part command or a stock write capability. The actual owner command still needs the exact original Workflow replay/input fingerprint, Assets capability admission (`settle-existing`, or History for an exact prior receipt), Inventory reservations/date/Quality checks, Finance and Money effects, and exact final source/event/stock/lot/journal after-images in one transaction. Full purchase source images must not be replaced by this minimal reference. Notifications/outbox and source-version advancement remain part of that same unit.

Start/complete/cancel also require Assets' actual lifecycle/version/evidence effects. Preventive plans, creation/scheduling wire validation, queries/SSR, owner HTTP, jobs, notifications and browser journeys remain pending. No native Maintenance route is mounted and no business-owner completion is claimed.

## Component verification

The registered targets are `maintenance.rust-work-policy.unit`, `assets.rust-maintenance-source-postgres.integration`, and `maintenance.rust-work-source-postgres.integration`; People and Identity retain their separate current-reference targets. The Asset cases use disposable migrated PostgreSQL and cover minimal scope, local changes/transaction escape, actual tuple retention and old-snapshot40001. The Work target uses real retained Account/Holding authorization, actual worker/Asset rows, independent review versus exact owner, selected scope, expiry and final revalidation. These are component checks, not full migration acceptance.
