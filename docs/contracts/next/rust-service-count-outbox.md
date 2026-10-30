# Service Count outbox ownership

This development migration assigns the outbox events for Service Initial Physical Stock Counts to the shared Rust worker for registered Holdings. Ownership applies to every Company in the registered Holding and follows the retained Count source, not a Company supplied by the client. The worker is a modular-monolith entrypoint; it is not an owner-specific service.

The Inventory-owned SQL predicates are the common input to the Rust claimant and the legacy claimant's exclusion. A Count event with a valid retained Service source is eligible for native projection. A `flow.assign` event also needs one consistent `stock_count` Flow event and matching receipt. A consistent known non-Stock Flow remains with the legacy worker even when its record ID collides with a Service Count ID. Missing, conflicting, or unknown Flow identity for a colliding ID is an infrastructure integrity quarantine: the Rust worker records `SOURCE_AMBIGUOUS`, retries, and eventually dead-letters it without projecting it. An owned Count event with invalid source evidence is handled similarly with `SOURCE_INVALID`. Neither condition falls back to the legacy claimant.

Historical Flow identities are retained database evidence, checked for consistency, not retroactive cryptographic proof. Migration 1077 protects identity prospectively while allowing the established next-step progression. Preflight of historical evidence and qualification of the command-event cardinality boundary remain required before activation. The source records, receipts, audit, outbox, and migration history are retained.

The claimant uses a token-bound 90-second lease, renewed while a projection is in progress. It claims with `SKIP LOCKED`; completion, renewal and failure require the same token. Failures retry with bounded exponential delay and dead-letter after six attempts. The Holding read database commits the idempotent projection receipt and `work-items` checkpoint in one transaction before the write database acknowledges the event. A crash between the commits is recovered by replaying the receipt insertion. The worker does not create Count Work items from these events: existing Count Work is read from the current owner source.

Activation requires validated, distinct leased Holding write and read databases, exact migration inventories, matching native and legacy ownership predicates, a shared worker running for every registered Holding, and source-bound mixed-claim and failure/recovery evidence. A missing native worker leaves owned events pending; it does not transfer them back to the legacy worker. Current development checks do not establish production or staging activation.

## Leased development entrypoint

Build the shared binary with `private-field-api` (or the broader existing `private-development-api` profile), then run:

```sh
erpx-server service-count-worker --development-config /absolute/private/config.json
```

The file must have private permissions. It uses the same validated Control database, exact Holding registry and `fieldServiceInitialCounts` targets as the private API. Each enabled Holding additionally supplies `readDatabase` with its own `url` and `leaseToken`; write and read database leases must be distinct and their migration tracks must match. The configuration accepts only the owned leased development databases and local connection policy. This entrypoint is not a production configuration interface. Never copy operator credentials into a fixture, repository or image.

The worker opens no HTTP listener. SIGTERM stops the current iteration and cancels only active queries bearing that worker run's generated application names, using the same database role. Pool closure is bounded; independent application sessions and lock holders are left intact. An interrupted event keeps its token-bound lease until recovery; shutdown does not falsely acknowledge it.

Targeted real PostgreSQL checks cover source-aware mixed claims, invalid and ambiguous source quarantine, token renewal and stale completion, two-database crash recovery, concurrent claims and retry/dead-letter behavior. The actual CLI check also holds an independent SQL blocker while sending SIGTERM, verifies bounded successful exit, and confirms that the blocker survives and the event remains recoverable. These checks do not establish production activation or a completed end-to-end Field workflow.
