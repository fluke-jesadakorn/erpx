# CQRS/DDD refactor baseline

Recorded on 2026-08-05 before the bounded-context extraction. The worktree was
already intentionally dirty; the refactor must preserve all pre-existing
changes and may not use a destructive reset to manufacture a clean baseline.

## Source shape

- Console `main.rs`: 53,873 lines.
- Server `main.rs`: 27,652 lines.
- Worker `main.rs`: 1,908 lines.
- Application `lib.rs`: 2,921 lines.
- Platform protobuf source: 4,336 lines.

## Verification baseline

- `cargo fmt --all --check`: passed (2.99 seconds).
- `cargo check-fast`: passed (74.62 seconds for the recorded target baseline).
- `cargo test-fast`: passed (80.84 seconds; ignored tests retained their
  existing explicit database requirements).

The pre-cutover SHA-256 of `.runtime/n8n/.n8n/config` is retained outside
application data and must remain:

`bfe114069c47d741f8e8e2de89dfe857bc6f9064bf2a1e31e348eb2215656cf6`

Further warm-build, Dioxus bundle, startup, request-latency, and projection-lag
measurements are recorded by the performance gate after the mechanical
extraction has produced stable measurement entry points.

## Stable post-extraction measurement points

Recorded after Wave 2 registration, with the development runtime already warm:

- Median warm `cargo check-fast`, five runs: **5.25 seconds**
  (6.14, 5.24, 5.25, 5.46, 5.24).
- Median warm `cargo test-fast`, five runs: **3.97 seconds**
  (4.05, 3.97, 3.99, 3.97, 3.97).
- Dioxus web build: **14.66 seconds**.
- Web output: **53,924,547 bytes raw** and **8,897,501 bytes gzip** for the
  generated HTML, JavaScript, CSS, and WebAssembly assets. The dominant debug
  WebAssembly asset is 53,773,409 bytes.
- Warm HTTP health p95, 40 loopback requests: **0.559 ms**.
- Warm Console HTML p95, 40 loopback requests: **0.402 ms**.

Authenticated representative query/render and projection-lag measurements
remain cutover gates because those require the new Quality transport and
read-side checkpoint contract to be active. The values above are the regression
baseline for the 10% warm-build/runtime and 5% bundle-size limits.

## Wave 3 Quality pilot gate

Recorded after the typed Quality command/query cutover, SQLx adapter,
transactional outbox projector, Server/Worker registration, and Console
contract update:

- Median warm `cargo check-fast`, five runs: **2.43 seconds**
  (6.34, 2.39, 2.43, 2.48, 2.37).
- Median warm `cargo test-fast`, five runs: **4.03 seconds**
  (4.14, 4.00, 4.04, 4.03, 4.01), a 1.5% increase from the stable
  post-extraction median and within the 10% gate.
- Dioxus web build: **16.04 seconds**, 9.4% above the 14.66-second baseline
  and within the 10% gate.
- Web output: **54,352,730 bytes raw**, **8,959,253 bytes gzip**, and
  **54,201,592 bytes WebAssembly**. Raw and compressed growth remain below 1%
  and within the 5% gate.
- `cargo fmt --all --check`, `cargo check-fast`, `cargo test-fast`,
  `domain_contracts`, `ui_domains`, `infra_holding_runtime`, and the CQRS/DDD
  architecture scan passed.
- The Quality SQLx acceptance target compiles and covers atomic rollback,
  stable replay, no write fallback, receipt-to-ReadYourWrites, duplicate
  projection, and shadow rebuild activation. Its live database path
  self-skipped because the two disposable holding test URLs were not exported
  in this shell. A bounded attempt to create two acceptance-only databases was
  rejected because the configured development role has neither `CREATEDB` nor
  superuser capability; no application database was changed. Fresh-state
  cutover still requires the live execution with provisioned test databases.
- The approved protobuf descriptor remains
  `d51af063b04b70093a1ed057d179ce0d282f1cedf2f0b0480ac138bebed8ea4b`.
- A selective `cargo xtask dev --detach` warm restart completed successfully;
  Server, workers, UCS labs, and the Dioxus web runtime were healthy and the
  development endpoint remained available on port 3004.
- Post-restart loopback measurements over 50 requests were **0.333 ms median /
  0.385 ms p95** for `/health` and **0.369 ms median / 0.513 ms p95** for the
  Console HTML. The health endpoint improved from the Wave 2 p95; the Console
  HTML p95 increased by 27.6%. Both remained sub-millisecond, but the relative
  Console change exceeds the 10% runtime budget and must be remeasured during
  the final representative authenticated render gate before cutover.
- The retained n8n configuration hash remains
  `bfe114069c47d741f8e8e2de89dfe857bc6f9064bf2a1e31e348eb2215656cf6`.

## Foundation dependency closure

Before Wave 4, the remaining Server-to-Worker dependency was removed. Shared
directory authentication/secret resolution now belongs to `erpx-identity`;
Finance import queue contracts and fail-closed runtime readiness belong to
`erpx-infrastructure`. Worker readiness uses the same shared definitions, and
the architecture scan now rejects both an `erpx-worker` Server dependency and
`erpx_worker::` imports under Server sources.

After the extraction, `cargo check` passed independently for Identity,
Infrastructure, Worker, and Server. `cargo fmt --all --check`, `cargo
check-fast`, `cargo test-fast`, and `cargo xtask architecture` also passed.

## Wave 4 foundational contexts gate

Identity & Access, Organization, and Commercial now have isolated core crates,
SQLx adapters, typed command/query protobuf services, rebuildable projections,
Server transports, Worker registrations, and transport-neutral Console feature
contracts. The three core crates contributed 40 focused domain/application
tests (12 Identity & Access, 14 Organization, and 14 Commercial).

- The approved protobuf descriptor is
  `5b24412ece62fbefee3f2686284b35780173afb5f1728bf29d39357ffa7a1710`.
- Commercial catalog state is platform-owned in the control write/read planes;
  holding subscriptions and entitlements remain holding-owned. Canonical
  migrations `0075`/`0015` create the control CQRS foundation and catalog
  models, while holding migrations `0058`/`0050` plus `0059`/`0051` create the
  context models and remove the superseded holding catalog tables.
- All four migration files executed successfully against the configured local
  PostgreSQL server inside one transaction and were rolled back. This validated
  syntax and ordering without retaining database changes.
- The Commercial provider ingress is server-only, HMAC authenticated, and
  binds the target aggregate, scope, and complete evidence before an internal
  verification token is injected. Console contracts cannot construct provider
  evidence.
- Server and Worker composition owns pool routing outside transport modules.
  Server no longer imports Worker runtime, and the architecture scan rejects
  pool construction in context transports and forbidden framework or
  cross-context dependencies in cores.
- `cargo fmt --all --check`, `cargo check-fast`, `cargo test-fast`, and
  `cargo xtask architecture` passed. The `identity_core`,
  `identity_acceptance`, `infra_holding_runtime`, `worker_platform`,
  `ui_domains`, and `domain_contracts` harnesses also passed; database-backed
  cases with explicit disposable-database requirements remained ignored.
- Focused verification passed for all Worker targets (375 library tests plus
  23 binary tests, with the existing explicit integration ignores), the
  Commercial SQLx adapter (13 tests), and Server/Worker all-target compilation
  without warnings.
- A selective `cargo xtask dev --detach` warm restart completed successfully;
  the recorded runtime services are healthy, the Console remains available on
  port 3004, and the retained n8n configuration hash is unchanged at
  `bfe114069c47d741f8e8e2de89dfe857bc6f9064bf2a1e31e348eb2215656cf6`.
- Median warm `cargo check-fast`, five runs: **5.54 seconds**
  (7.64, 5.83, 5.52, 5.53, 5.54), 5.5% above the stable post-extraction
  median and within the 10% gate. Median warm `cargo test-fast`: **4.19
  seconds** (5.16, 4.27, 4.04, 4.03, 4.19), 5.5% above the same baseline and
  within the 10% gate.
- The first Dioxus build after target invalidation took 33.40 seconds; the
  immediately repeated warm build took **5.00 seconds**. The web output is
  **54,619,032 bytes raw**, **8,988,577 bytes gzip**, and **54,467,894 bytes
  WebAssembly**. Raw and compressed growth from the Wave 3 gate remain below
  1% and within the 5% bundle limit.
- Post-restart loopback p95 over 50 requests was **0.425 ms** for Server health
  (0.348 ms median) and **0.450 ms** for Console HTML (0.388 ms median). Both
  remain sub-millisecond. Health is 10.4% above the Wave 3 sample but below the
  stable post-extraction baseline; Console improved from Wave 3 but remains
  11.9% above the stable post-extraction sample. Representative authenticated
  query/render measurement therefore remains a final cutover gate rather than
  being inferred from these unauthenticated loopback samples.

## Wave 5 control/customer contexts gate

Platform Administration, Brand, and Customer Authority now have isolated core
crates, control-plane SQLx adapters, typed command/query protobuf services,
rebuildable projections, Server composition, Worker catalog registration, and
transport-neutral Console feature contracts.

- The approved protobuf descriptor is
  `a04ac8520d389ccc90b812ff42908910da6721110cbcda13ca060f4df8350256`.
- Canonical control migrations `0076`/`0016` add the three contexts' guarded
  write models and generation-scoped read projections on top of the shared
  `0075`/`0015` CQRS foundation. Both files executed successfully against the
  configured control write/read databases inside separate transactions and
  were rolled back, so syntax and ordering were checked without retaining
  database changes.
- Platform Administration requires platform authority and fresh MFA for every
  high-risk write. Brand validates exact scope, parent publication witnesses,
  immutable asset references, and activates the profile generation before a
  resolved-brand rebuild. Customer Authority keeps registration/tax identity
  out of read projections and atomically guards merge-survivor version and
  non-merged/non-churned status.
- Server composition registers six typed Wave 5 services without constructing
  storage in transport modules. Worker composition owns control pools, runs all
  seven projectors incrementally, and supports a one-shot full rebuild through
  `ERPX_WAVE5_PROJECTION_MODE=rebuild`. Console presenters/views remain free of
  protobuf and aggregate imports and expose loading, submitting, empty,
  success, denied, stale, conflict, offline, not-ready, and failed states.
- Focused tests passed for all three cores/adapters (80 tests), Console (197),
  Server (129 passed, one explicit disposable-database ignore), Worker library
  (375 passed, three explicit integration ignores), and Worker binary (35).
- `cargo fmt --all --check`, `cargo check-fast`, `cargo test-fast`, and
  `cargo xtask architecture` passed. The `infra_platform`, `worker_platform`,
  and `ui_platform` harnesses also passed; their documented disposable-database
  cases remained ignored.
- Median warm `cargo check-fast`, five runs: **5.87 seconds**
  (7.33, 5.95, 5.87, 5.81, 5.81), 5.9% above the Wave 4 median and within the
  10% gate. Median warm `cargo test-fast`: **4.08 seconds**
  (4.08, 4.06, 4.12, 4.09, 4.07), an improvement from Wave 4.
- The first Dioxus build after the UI workflow registry changed took 14.39
  seconds; the immediately repeated warm build took **4.72 seconds**. Web
  output is **55,067,450 bytes raw**, **9,058,273 bytes gzip**, and
  **54,916,312 bytes WebAssembly**. Growth from Wave 4 is below 1% and remains
  within the 5% bundle gate.
- A selective `cargo xtask dev --detach` restart rebuilt and restarted Server,
  workers, and UCS labs while reusing the Dioxus hot-reload process. Runtime
  status was healthy, Console returned HTTP 200 on port 3004, and the retained
  n8n configuration hash remained
  `bfe114069c47d741f8e8e2de89dfe857bc6f9064bf2a1e31e348eb2215656cf6`.
- Post-restart loopback p95 over 50 requests was **0.379 ms** for Server health
  (0.351 ms median) and **0.429 ms** for Console HTML (0.397 ms median), both
  improved from Wave 4 and within the runtime regression gate.

## Wave 6 paired-context Phase A and barrier

Marketing/CRM, Procurement/Inventory, and Assets/Maintenance now have isolated
core crates with domain invariants, typed command/query handlers, ports, and
owner-only integration mappings. Consumers depend on published contract facts,
never the source context aggregate.

- All six core crates passed 62 focused domain/application tests and the
  CQRS/DDD architecture scan. Tests cover authority/entitlement, optimistic
  concurrency, stable idempotency, projection-only ReadYourWrites, lifecycle
  invariants, and typed cross-context handoffs.
- The direct-v1 barrier adds typed command/query services for all six contexts.
  CRM's `Discovery` opportunity stage was caught by barrier review and added to
  both transport and persistence contracts before adapters were implemented.
- Holding-write migration `0060` adds context guards, movement/reservation
  uniqueness, and Maintenance's typed Assets fact inbox. Holding-read migration
  `0052` adds generation-scoped disposable projections for all six contexts.
- The approved barrier descriptor is
  `530757265c916b8fb68a3b50e6fca945e48d8e1f2b990104f70b91d71a9419cf`.
- Protobuf generation, migration registry compilation, UI descriptor/service
  inventory, human route/adapter/state parity, formatting, and the architecture
  scan passed before Phase B began.

## Wave 6 paired-context Phase B gate

Marketing/CRM, Procurement/Inventory, and Assets/Maintenance now complete their
end-to-end context packets with SQLx adapters, atomic command units of work,
generation-scoped projectors, typed Server transports, Worker registrations,
and transport-neutral Console gateways, presenters, and views.

- Server composition owns pool routing and registers all 12 typed command/query
  services. Route scope is checked against the authenticated holding scope; the
  Marketing and CRM transports include regressions for holding-scope mismatch.
  Context transports do not construct pools or import stores.
- Worker composition registers seven rebuildable snapshot projectors plus the
  Assets-to-Maintenance fact consumer. That consumer validates outbox metadata,
  decodes the typed integration envelope, preserves a per-holding cursor during
  the process lifetime, and safely replays from sequence zero after restart.
  Duplicate facts are idempotent and same-version conflicts are rejected.
- Console feature-local UI DTOs isolate presenters and views from protobuf,
  Tonic, and context aggregates. Core-to-UI translation is confined to the
  context gateways, and the shared workflow inventory contains all 12 Wave 6
  services.
- Focused core and adapter verification passed **95 tests**. Server passed
  **141 tests** with one explicit disposable-database ignore; Worker passed
  **375 library tests** with three explicit integration ignores plus **45
  binary tests**; Console passed **225 tests**. The `domain_contracts` (31),
  `ui_domains` (20), `infra_migration`, and `worker_platform` harnesses passed,
  with only their documented disposable-database cases ignored.
- `cargo fmt --all --check`, `cargo check-fast`, `cargo test-fast`, and
  `cargo xtask architecture` passed. The architecture scan confirms that core
  contexts have no framework or cross-context core imports, transports have no
  store construction, and Console presenters/views have no domain/protobuf
  imports.
- Median warm `cargo check-fast`, five runs: **6.03 seconds**
  (7.68, 6.03, 6.15, 6.00, 6.01), 2.7% above Wave 5 and within the 10% gate.
  Median warm `cargo test-fast`: **4.12 seconds**
  (4.22, 4.12, 4.16, 4.07, 4.11), 1.0% above Wave 5 and within the gate.
- The first Dioxus build after Wave 6 invalidation took 28.78 seconds; the
  immediately repeated warm build took **5.05 seconds**, 7.0% above Wave 5 and
  within the warm-build gate. Web output is **55,557,210 bytes raw**,
  **9,127,828 bytes gzip**, and **55,406,072 bytes WebAssembly**. Raw and
  compressed growth from Wave 5 remain below 1% and within the 5% bundle gate.
- The approved protobuf descriptor remains
  `530757265c916b8fb68a3b50e6fca945e48d8e1f2b990104f70b91d71a9419cf`.
  Holding migrations `0060`/`0052` remain the canonical Wave 6 write/read
  schema allocation.
- A selective `cargo xtask dev --detach` restart completed successfully.
  Server, workers, UCS labs, and Dioxus are recorded healthy; Console returned
  HTTP 200 on port 3004. Over 50 Console requests the median was **0.393 ms**
  and p95 was **0.440 ms**, 2.6% above Wave 5 and within the runtime gate.
- The retained n8n configuration hash is unchanged at
  `bfe114069c47d741f8e8e2de89dfe857bc6f9064bf2a1e31e348eb2215656cf6`.

## Wave 7 transactional contexts Phase A and barrier

Sales, People & Payroll, and POS now have isolated core crates with typed
aggregates, application commands/queries, authorization and entitlement ports,
atomic unit-of-work contracts, projection-only read ports, and owner-scoped
integration mappings.

- The three cores passed **43 focused tests**: Sales 13, People & Payroll 18,
  and POS 12. Coverage includes lifecycle and amount invariants, actor
  separation, expected versions, stable idempotency, authorization and scope,
  atomic commit requests, typed outbox facts, and ReadYourWrites not-ready
  without a write-store fallback.
- Sales consumes only CRM's published `OpportunityFact` and accepts it only at
  Won; Quote remains private and only a SalesOrder fact is published. People &
  Payroll publishes employee status and finalized payroll facts while leave
  and expense events remain private. POS publishes completed-sale and approved-
  refund facts; the unused settlement contract remains forward-compatible and
  no unsupported settlement lifecycle was invented.
- The direct-v1 barrier adds `SalesCommandService`/`SalesQueryService`,
  `PeoplePayrollCommandService`/`PeoplePayrollQueryService`, and
  `PosCommandService`/`PosQueryService` with typed payloads and common CQRS
  metadata/receipts. Legacy generic services remain descriptor-visible only
  until Phase B completes its coordinated Server/Worker/Console cutover.
- Holding-write migration `0061` adds command guards for all three contexts;
  holding-read migration `0053` adds eight generation-scoped projections:
  Sales quotes/orders, People employees/leave/expenses/payroll runs, and POS
  sales/refunds. Both migrations executed inside PostgreSQL transactions and
  were rolled back; the read test supplied only a transactional stub for the
  shared projection-generation table because the selected validation database
  is not the holding read database.
- Protobuf generation, typed RPC workflow inventory, migration registry test,
  formatting, all three core targets, and the CQRS/DDD architecture scan
  passed. No core imports SQLx, Tonic, Prost, Dioxus, or another context core.
- The approved barrier descriptor is
  `61ca5a8ec44ac8a18a6a5d21284544de4d9f0cd089729b6ddf97361bbbd0fe70`.

## Wave 7 transactional contexts Phase B gate

Sales, People & Payroll, and POS now complete their end-to-end context packets
with atomic SQLx command adapters, projection-only read repositories, eight
generation-scoped projectors, typed Server transports, Worker scheduler
registrations, and transport-neutral Console features.

- Server composition routes each authenticated holding command to the write DB
  and each query to the read DB, then registers all six typed command/query
  services. Sales uses `sales.read`/`sales.write`, People & Payroll uses the
  existing `hr.read`/`hr.write` authority, and POS uses `pos.read`/`pos.write`.
  Route holdings are bound to authenticated holding scope before dispatch.
- Worker composition registers `sales_quotes`, `sales_orders`, four
  `people_payroll_*` projections, `pos_sales`, and `pos_refunds`. Incremental
  execution and shadow rebuild/activation share the CQRS checkpoint, retry,
  projection-receipt, and dead-letter framework.
- POS cumulative refund protection locks the sale row as a serialization fence;
  requested, approved, and posted refunds reserve balance, while rejected and
  cancelled refunds release it. This prevents concurrent over-refunds without
  exposing the write DB to queries.
- Console gateways are the sole core-to-UI translation boundary. Presenters and
  views contain no protobuf, Tonic, or aggregate imports and expose loading,
  submitting, empty, success, denied, stale, conflict, offline, not-ready, and
  bounded failure states.
- The three context cores and SQLx adapters passed **72 focused tests** (43 core
  and 29 adapter). Server passed **150 tests** with one documented disposable-
  database ignore; Worker passed **375 library tests** with three documented
  ignores plus **55 binary tests**; Console passed **240 tests**. The
  `domain_contracts` (31), `ui_domains` (20), `infra_migration`, and
  `worker_platform` harnesses passed with only their documented disposable-
  database cases ignored.
- `cargo fmt --all --check`, `cargo check-fast`, `cargo test-fast`, and
  `cargo xtask architecture` passed. The migration registry assertion for
  holding-write `0061` and holding-read `0053` passed after the payroll approved/
  posted actor guards were corrected.
- A Cargo fingerprint audit found that nested `xtask` invocations inherited
  the outer package's `CARGO_MANIFEST_DIR`, alternating build-script cache keys
  with direct Cargo runs. The lightweight runner now removes that inherited
  value before spawning Cargo. Wave 7 adapters also keep migration SQL constants
  test-only instead of embedding them in production metadata.
- Median warm `cargo check-fast`, five runs after stabilization: **6.517
  seconds** (6.637, 6.824, 6.517, 6.504, 6.403), 8.1% above Wave 6 and within
  the 10% gate. Median warm `cargo test-fast`: **4.215 seconds** (5.871, 4.188,
  4.371, 4.187, 4.215), 2.3% above Wave 6 and within the gate.
- The first Dioxus build after Wave 7 invalidation took 44.657 seconds; the
  immediately repeated warm build took **5.358 seconds**, 6.1% above Wave 6
  and within the warm-build gate. Web output is **56,051,594 bytes raw**,
  **9,179,474 bytes gzip**, and **55,900,456 bytes WebAssembly**. All bundle
  growth from Wave 6 is below 1% and within the 5% gate.
- The approved protobuf descriptor remains
  `61ca5a8ec44ac8a18a6a5d21284544de4d9f0cd089729b6ddf97361bbbd0fe70`.
  Holding migrations `0061`/`0053` remain the canonical Wave 7 write/read schema
  allocation.
- A selective `cargo xtask dev --detach` restart rebuilt and restarted Server,
  workers, and UCS labs while reusing Dioxus hot reload. Runtime status is
  healthy, Console returns HTTP 200 on port 3004, and a stabilized 50-request
  Console run recorded p95 **0.480 ms**, within 10% of Wave 6. Server health
  recorded median 0.368 ms and p95 0.431 ms.
- The retained n8n configuration hash is unchanged at
  `bfe114069c47d741f8e8e2de89dfe857bc6f9064bf2a1e31e348eb2215656cf6`.

## Wave 8 dependent contexts Phase A and barrier

Finance, Legal, and Support now have isolated, framework-free core crates with
typed commands and queries, explicit authorization/entitlement ports, atomic
unit-of-work contracts, projection-only reads, and owner-scoped integration
mappings.

- The three cores passed **47 focused tests**: Finance 15, Legal 18, and
  Support 14, including their architecture suites. Coverage includes balanced
  journals, close approval separation, subledger settlement bounds, Legal
  retention and append-only evidence, Support audience separation, MFA/SoD,
  capability redaction, stable replay, atomic commit shape, and ReadYourWrites
  not-ready without write fallback.
- Finance consumes Sales, Inventory, People & Payroll, POS, and Procurement
  only through typed integration facts. Monetary posting rules and account
  mappings are not inferred. Source-derived receivable/payable creation stays
  behind a process-manager boundary instead of accepting client-supplied facts
  through gRPC.
- Legal contracts, documents, and access evidence remain private because the
  existing `LegalObligationFact` describes a different case-obligation
  semantic. Support adds a secret-free `SupportIncidentFact` for Operations;
  sessions, messages, approvals, timelines, and capabilities are not published.
- The direct-v1 descriptor now contains typed Finance, Legal, and Support
  command/query services. Support customer and internal ticket schemas are
  physically distinct. No typed Support request or response contains a bearer,
  credential, or secret hash, and the requesting support agent is derived from
  authenticated command metadata.
- Holding-write migration `0062` adds Finance, Legal, and Support guards;
  holding-read migration `0054` adds fourteen generation-fenced projections.
  Finance and Legal evidence tables are append-only, Support capability hashes
  exist only on the write side, and Support customer/internal message
  projections are separate tables.
- Both migrations executed successfully against isolated temporary PostgreSQL
  databases (18 write-side and 16 read-side tables) and the temporary databases
  were removed afterward. Descriptor generation, RPC workflow inventory,
  migration registry privacy/separation test, formatting, core tests, and the
  integration-contract tests passed.
- The frozen Wave 8 barrier descriptor is
  `34da323cdfa1c092601a7494cdc865be94e1cb6c707f3c4a8b53b784f7bacd22`.

## Wave 8 dependent contexts Phase B gate

Finance, Legal, and Support now complete their end-to-end packets with atomic
SQLx command adapters, projection-only query adapters, fourteen rebuildable
read projectors, typed Server transports, Worker scheduler registrations, and
transport-neutral Console features.

- Finance publishes and projects seven independent read models, Legal three,
  and Support four. Every Wave 8 worker claims one exact dot-qualified
  projection name, rejects a mismatched claim before holding routing, and uses
  the durable rebuild UUID as the stable generation across retries.
- Support authorization now derives an exact action from the typed request,
  distinguishes customer-channel reads/replies from internal notes, and
  requires current MFA and separation-of-duties evidence for approval and
  capability operations. Customer queries cannot return another customer
  account or internal messages.
- Capability issuance is stable-idempotent without including rotating MFA
  evidence in its command fingerprint. A server-only HMAC escrow key derives
  the same bearer for a committed command retry; only its SHA-256 digest is
  stored. The raw 32-byte bearer is delivered once through binary response
  metadata after commit and is zeroized in memory. It never enters protobuf,
  Console models, read projections, audit payloads, or outbox facts.
- The v1 service fence permits exactly one Finance/Legal/Support authority
  family at runtime. Development remains on the legacy family until the
  coordinated typed Console cutover in Wave 10; both families cannot be
  registered together.
- Identity schema bootstrap now keeps its transaction-scoped advisory lock
  across the complete forward-migration set. This prevents concurrent Server
  and multi-projector startup from interleaving later migration files.
  Projection authorization resolves opaque registry holding references through
  the authoritative holding summary while preserving direct UUID bindings.
- The development fixture is versioned `erpx-demo-v4`; its versioned role
  assignment includes only the explicit `holding.projection.execute` capability
  required by the projector service principal. Existing application records
  and namespaces were not deleted or rewritten.
- Fast-lane verification passed after runtime hardening:
  `cargo fmt --all --check`, `cargo check-fast`, `cargo test-fast`, and
  `cargo xtask architecture`. Focused Support, CQRS SQLx, Server all-target,
  Worker all-target, infrastructure scheduler, and xtask lifecycle tests also
  passed. Relevant infrastructure, Finance, People/Support,
  Finance/Operations, domain-contract, and UI-domain harnesses passed with only
  their documented disposable-database ignores.
- Median warm `cargo check-fast`, five runs: **6.69 seconds**
  (10.76, 6.48, 6.60, 6.99, 6.69), within the 10% gate from Wave 7. Median
  warm `cargo test-fast`: **4.21 seconds** (5.50, 4.25, 4.21, 4.19, 4.17).
  The invalidated Dioxus build took 44.25 seconds and the immediate warm build
  **4.65 seconds**. Web output is **56,720,689 bytes raw**, **9,290,427 bytes
  gzip**, and **56,569,551 bytes WebAssembly**, approximately 1.2% above Wave 7
  and within the 5% bundle gate.
- A selective `cargo xtask dev --detach` restart completed successfully. The
  lifecycle catalog now reports and manages Server, Console, UCS, all fourteen
  Wave 8 projectors, the Support worker, and four dedicated workers. Server
  health p95 over 50 requests was **0.448 ms** and Console p95 on port 3004 was
  **0.489 ms**.
- The retained n8n configuration hash is unchanged at
  `bfe114069c47d741f8e8e2de89dfe857bc6f9064bf2a1e31e348eb2215656cf6`.

## Wave 9 composed contexts Phase A and barrier

Operations and Customer 360 now have framework-free cores and frozen typed
contracts for the composed/orchestration wave.

- Operations accepts source facts only through a worker-owned typed process
  manager. Its public command surface contains exception lifecycle and
  holding-owned work-queue policy commands; no JSON source mutation or source
  authority proof is accepted from a client. POS ingestion matches the two
  facts POS actually publishes (`sale_completed` and `refund_approved`), and
  Legal is not advertised until Legal publishes a compatible fact.
- Customer 360 is a read-only composition of Customer, Commercial, CRM, Sales,
  and Support facts. The control-customer and holding-activity streams keep
  independent positions, expose both freshness states, and report seven
  unsupported legacy sections as `not_projected` instead of guessing data.
- The CQRS SQLx foundation now has an additive multi-source runner with one
  cursor per physical source, holding-qualified partitions, compare-and-swap
  advancement, per-source rebuild high-watermarks, DLQ-aware activation, and
  fair page rotation. The existing single-source runner is unchanged.
- Holding-write migration `0063` owns Operations guards/source receipts/source
  checkpoints; holding-read migration `0055` owns its generation-fenced read
  models. Control-read migration `0017` owns the composed Customer 360 tables
  and source-qualified cursors. No global outbox sequence is compared across
  databases.
- The Phase A focused gate passed **57 tests**: CQRS SQLx 23, Operations 19,
  Customer 360 9, and integration contracts 6. Protobuf generation passed with
  the physical `customer_360.proto` split.
- The frozen Wave 9 barrier descriptor is
  `a91940c11a7c7568b17469576bfec6e5148e8530bef5b3b8b57bec9c3d5c3521`.

## Wave 9 composed contexts Phase B gate

Operations and Customer 360 now complete their end-to-end packets with typed
Server transports, SQLx command/query adapters, durable projectors, production
Worker registrations, transport-neutral Console gateways/presenters, and
focused contract tests.

- Operations atomically commits aggregate guards, stable receipts, audit,
  outbox, source receipts, and source checkpoints. The production source
  process manager scans only the closed typed event inventory and delegates to
  the Operations application handler; automatic detection is disabled by
  default and, when enabled, requires a configured Identity-backed manager
  chain and explicit bounded policy fields.
- Three Operations projections and the Customer 360 control/holding fan-in
  projections support durable retry, DLQ, checkpoint, restart, and rebuild
  semantics. Rebuild activation rejects unresolved failures, resumes persisted
  cursors/fences, and clears pending state only after successful activation.
- Customer 360 control projection execution is physically separated from
  holding enumeration. Pre-cutover opaque holding registry identifiers are
  converted to stable projection UUIDs without mutating the retained registry;
  fresh UUID registry identifiers pass through unchanged.
- The direct-v1 workflow inventory now classifies every typed Operations and
  Customer 360 RPC and binds each human method to a typed Console adapter,
  route, action capability, and the complete visible-state contract.
- Architecture scanning, focused core/adapter/Server/Worker/Console tests,
  Server/Worker/Console all-target checks, `infra_migration`, and the real
  disposable PostgreSQL migration acceptance passed. The acceptance migrated
  and removed six isolated databases and did not cut over the development
  resources.
- The Cargo fast lane now strips all package-scoped variables inherited from
  its outer `cargo run` and uses an isolated `.runtime` target. This prevents
  direct Cargo, Dioxus, and nested Cargo fingerprints from invalidating one
  another. Median warm `cargo check-fast`, five runs: **0.56 seconds** (0.58,
  0.56, 0.56, 0.55, 0.56). Median warm `cargo test-fast`: **4.11 seconds**
  (4.17, 4.14, 4.11, 4.11, 4.11), an improvement from Wave 8.
- The invalidated Dioxus build took **14.09 seconds** and the immediate warm
  build **4.67 seconds**, 0.4% above Wave 8. Web output is **57,221,283 bytes
  raw**, **9,342,990 bytes gzip**, and **57,070,145 bytes WebAssembly**; growth
  remains below 1% and within the 5% bundle gate.
- A selective `cargo xtask dev --detach` restart completed successfully. All
  three Operations projectors, Operations source ingestion, and both Customer
  360 projectors are running. Server health p95 over 50 requests was **0.423
  ms** and Console p95 on port 3004 was **0.418 ms**.
- The retained n8n configuration hash is unchanged at
  `bfe114069c47d741f8e8e2de89dfe857bc6f9064bf2a1e31e348eb2215656cf6`.

## Wave 10 final composition and direct-v1 cutover

The CQRS/DDD cutover remains a modular monolith. The workspace now owns 21
framework-free context crates, one SQLx adapter per context plus the shared
CQRS SQLx foundation, and 25 physically split platform protobuf files. Context
cores do not depend on SQLx, Tonic, Prost, Dioxus, or another context core;
cross-context traffic is expressed through owner-namespaced integration facts
and process managers.

- The Server, Worker, and Console entrypoints are now 4, 6, and 3 lines
  respectively and delegate to composition modules. Server no longer imports
  Worker runtime construction. Console feature presenters/views are wire-free,
  while transport modules are the only generated-protobuf mapping boundary.
- The direct-v1 descriptor hash is
  `9e8b87195ffce4108c8cdd3fc62f61f5b036f67f26117193155b17faa6912417`.
  The runtime and Console authority are both `typed`; legacy generic services
  and routes are fail-closed in this authority mode.
- The retained pre-cutover release is
  `.runtime/erpx/cutover/releases/pre-fresh-state-2026-08-05/` (309 MiB),
  including Server/Worker/UCS binaries, the Console web output, hashes, and a
  private environment snapshot. Its resources remain
  `offline_non_authoritative` with `isolated_no_merge` write policy.

### Fresh-state evidence

- The authoritative installation prefix is `retired-installation`, generation
  `a2b0b92a-12b5-51ad-aa92-41065d9175d7`, holding
  `4017ec64-6620-50e2-bfdc-14d0a9b3c414`, and holding
  `c926087d-100d-52b9-b3b3-b27f0b34a1e0`.
- Fresh resources are `retired-installation_control_write`, `retired-installation_control_read`,
  `retired-installation_4017ec646620_holding_write`,
  `retired-installation_4017ec646620_holding_read`, and the
  `retired-holding-objects` MinIO namespace. No prior identity, RBAC, business,
  or object records were copied.
- Bootstrap produced exactly two seed principals, two seed roles, twelve
  canonical business units, one neutral-brand guard/revision, and 22 initial
  projection jobs. Credential and progress artifacts are mode `0600`; the
  progress marker is terminal `complete`.
- The first isolated `retired-installation` attempt failed closed before authoritative
  cutover because the old reference validator rejected the namespaced holding
  owner principal. Its marker and unused credential artifact were moved to
  `.runtime/erpx/bootstrap/failed/6a36faf3-61b9-58cc-b40e-39d153606971/`;
  its partial empty resources were retained and never adopted or deleted.
- The bootstrap now creates the customer root-scope binding and ERPX-managed
  holding infrastructure profile itself. The already-created `retired-installation`
  installation received the same two rows in one atomic reconciliation, and
  the seven route-related dead letters were requeued exactly once with an
  audit event. All 22 initial projection jobs subsequently succeeded.
- Startup waits for all 54 Worker heartbeats and all bootstrap projection jobs
  before starting Server. The typed neutral-brand smoke then observed exactly
  two command receipts, two audit rows, three outbox events over two command
  identities, one active read projection, two projection receipts, and a read
  checkpoint at the outbox high-watermark without write-side fallback.
- The n8n configuration remains byte-identical at
  `bfe114069c47d741f8e8e2de89dfe857bc6f9064bf2a1e31e348eb2215656cf6`.

### Final verification and performance evidence

- `cargo fmt --all --check`, `cargo check-fast`, `cargo test-fast`, the
  architecture scan, all-target Server/Worker/Console checks, the Dioxus web
  build, and every relevant domain/infrastructure/worker/migrator/UI harness
  passed. Acceptance scans now follow the Wave 10 owner modules rather than
  searching the thin entrypoints. The serialized final command is
  `cargo xtask verify`.
- A clean isolated `cargo check-fast` took **64.55 seconds**. Five warm check
  runs were **0.59, 0.55, 0.55, 0.54, and 0.54 seconds** (median **0.55**,
  1.8% faster than Wave 9). Five warm test runs were **4.32, 4.29, 4.29, 4.31,
  and 4.41 seconds** (median **4.31**, 4.9% slower than Wave 9 and within the
  10% gate).
- The invalidated Dioxus build took **39.16 seconds** and the immediate warm
  build **4.75 seconds**, 1.7% above Wave 9 and within the 10% warm gate. Web
  output is **61,084,060 bytes raw**, **9,555,225 bytes gzip**, and
  **60,932,922 bytes WebAssembly**. Compressed transfer size grew 2.27% and
  passes the 5% gate; raw and WebAssembly artifacts grew 6.75% and 6.77%, so
  those two non-transfer measurements remain recorded exceptions.
- Median-of-five 50-request p95 batches measured Server health at **0.504 ms**
  and Console at **0.564 ms**. Both remain sub-millisecond, but the relative
  changes from Wave 9 (**0.423 ms** and **0.418 ms**) exceed 10%; this
  microbenchmark remains a recorded performance exception rather than being
  reported as a passing gate.
