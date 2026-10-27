# Native Legal processing and polling

This is an implemented library boundary under verification, not a mounted HTTP API or complete Legal migration. The [delivery plan](../../production-plan.md) and [owner register](../../status/rust-backend-migration.json) retain the remaining work.

## Current viewer reads

`LegalProcessingStatusService` composes current Identity work authorization, the Legal record policy, actual release observations and Commercial History. Its input contains an exact configured host, an inbound device proof, a workspace selection and a job identifier. A request cannot supply a serialized actor or select a database. The current Holding registry and Account/selected-role witnesses remain fenced through the Holding transaction and commit.

Polling is a read of the job's visible Legal item. It uses `Metadata` record policy, preserving the existing `processingStatus` application's item-access behavior. A current selected role must grant `legal.read`; Company/branch/department scope, confidential membership and requester ownership still apply. The item's capability requires retained History. A stopped NewWork purchase does not by itself remove History. Polling does not request a new processing purchase, read original document bytes or require the documents capability merely to return item metadata. Disabled Legal settings do not independently prevent an otherwise authorized read. Release behavior still follows the explicitly configured existing record policy.

The service obtains the job from its configured Company, authorizes that item's current policy, checks its retained source, decodes bounded result JSON and revalidates time-sensitive record/purchase policy before returning. The queued actor snapshot is neither selected nor parsed. No file access, scanner invocation, worker effect, audit write or outbox write occurs during polling.

The response retains the existing fields: `id`, `item_id`, `document_id`, `state`, `error`, `attempts`, `jobId` and `result`. Absent error/result values are JSON null. `actor_json`, `result_json`, leases, checksums and credentials are excluded. HTTP envelopes, private no-store caching and canonical destinations remain composition responsibilities when the router is implemented and verified.

## Bounded source retention

The Legal owner maintains one canonical job-status reader shared by its existing processing repository and the new service. PostgreSQL caps result JSON at3MiB, error text at64KiB and scalar identifiers/state before transferring them. An oversized payload remains an error; it is never presented as an absent result. Payload decoding/admission errors are delayed until current item access succeeds, so corrupt result bytes do not bypass confidential-item policy.

The source holds a row share lock and a private fingerprint of its bounded scalar/raw result fields. Its witness binds the actual database, postmaster start, backend and full transaction ID. A different transaction or local row rewrite cannot reuse it. It requires a real serializable transaction; it is not a permission token. Source locks are acquired before the owning transaction's first data snapshot. Neither the status source nor the record witness has a request-deserialization constructor.

## Verification scope

The registered `legal.rust-queued-authority-composition.integration` suite exercises actual current Identity, Legal record policy and Commercial sources in disposable PostgreSQL databases. The added polling cases cover the exact public response, null results, retained History, disabled-settings metadata reads, confidential membership withdrawal, assignment expiry, malformed/oversized payloads, transaction mismatch and local source rewrites. The final source-coherent invocation passes all19 cases, including the existing worker/queue cases, with zero failures or skipped cases. This is component verification, not browser/runtime acceptance.

The implementation is in [owner status source](../../../packages/server-legal/src/adapters/postgres_processing_status.rs), [public source facade](../../../packages/server-legal/src/public/processing_status.rs) and [service composition](../../../src/composition/src/legal_processing_status.rs). These component boundaries do not establish general Legal reads, full worker parity, browser polling, production readiness or completion of the TypeScript backend withdrawal.
