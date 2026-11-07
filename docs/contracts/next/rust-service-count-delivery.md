# Service Initial Physical Stock Count delivery boundary

This capability is a step in the Field bundle. It does not close any complete catalogue workflow or any of the 41 owners. Its integration destination is `codex/rust-backend-migration`; integration is not permission to deploy or merge into `development`.

## Native owner and entrypoints

Rust owns the Service Count selection/workspace, record and Flow projections, create, generic draft revision, submit, approve, reject, cancel, post, and primary Flow assignment including `saveDefault`. The existing Service UI and its Next BFF call the private Rust transport. Generic Stock Count mutation and Flow routes select a Service source using persisted owner evidence before opening a legacy command transaction. Missing configuration, transport failure or changed context fail closed. Ordinary Supply, Sales, POS and Manufacturing Counts retain their own implementations.

Ownership is Holding-wide across its registered Companies. Headers capture the rendered Company, assignment, revision and assignment version; they do not grant authority. The Account authorizer resolves the current opaque session and selection, enforces the registered Holding and rechecks the selected Company inside the command transaction. The former bootstrap-Company target is not the authority for this capability. A two-Company real Account/SSO test covers separated reads and writes, foreign references, stale or missing context, revoked assignments and Holding mismatch.

The native transaction preserves purchase purpose, current role, preparer/reviewer separation, source witnesses, idempotency, Flow/defaults, stock and valuation, Finance journal, audit and outbox behavior. Exact retries retain the original receipt without repeating effects. Historical TypeScript receipts are checked under the retained receipt-authoritative contract; defaults have no retrospective command provenance. Inputs remain bounded to 500 Count rows and 2 MiB, as documented by the adapters; these checks do not establish arbitrary unbounded parity.

## Legacy withdrawal and retained consumers

The replaced Service workspace/create/action/Flow implementation, its transaction wrapper, image helper, Service-only schemas/types and domain export are removed after the actual normal-authentication native journey gate. Behavioral assertions remain in native tests or the fixed historical interoperability oracle. Source-history helpers still used by other owners remain explicit dependencies.

| Retained implementation | Actual consumer and scope | Withdrawal condition |
| --- | --- | --- |
| Service Count ProductEntry read | Identity product-workspace entry under the actual Finance or Stock assignment | Replace the Identity public entry projection with equivalent native authority and qualify its consumers. It does not execute Count commands. |
| Service source rows, marker/history and source-image reads | Inventory purchase guards, Finance source presentation, Files attachment guards and other owner lineage | Move each consumer to an equivalent public owner contract; preserve stored receipts and source history. |
| Mixed Count list/options and transactional consumer projections | Mixed Supply inventory lists/options, staff visibility, Quality and Finance transaction-local source reads | Supply a merged owner-aware list/order contract and equivalent transaction-local owner interfaces before removing these remaining shared reads. Service generic detail GET/SSR and Flow GET/SSR already use a typed native projection. These remaining paths do not own migrated Count commands. |
| Shared ordinary Count application and schemas | Supply, Sales, POS, Manufacturing and quality-scrap operations | Migrate each remaining owner capability; the source-aware Service fence remains mandatory meanwhile. |
| Files upload/scan/content and attachment purchase interfaces | Shared Files owner and optional evidence selection | Migrate the Files capabilities separately. Native Count submit validates and binds its own supplied evidence; retaining the shared Files owner does not allow a second Count writer. |
| Legacy shared jobs runtime | Event families outside Service Count ownership | Retire each event family when its native worker is qualified; current Service Count and ambiguous colliding events are excluded from legacy claims. |

The remaining mixed-owner and transaction-local shared reads above are declared transitional Inventory read dependencies, not a claim that all generic Inventory rendering or Files runtime has migrated. They retain no replacement Service command implementation. A transaction-attested source fence prevents Service Count mutation through the shared TS application, including internal callers, retries and stale-snapshot birth races. A disabled purchase profile does not bypass it.

No data, Count, audit, outbox, migration history, receipts or dated evidence is deleted by this implementation retirement. Migration 1077 adds prospective Flow identity and assignment-cardinality protection. Historical inconsistent events are retained and quarantined rather than rewritten as valid provenance.

## Jobs, verification and container boundary

The [shared native Count worker](rust-service-count-outbox.md) exclusively claims the owned Count/Flow event subset. It commits a projection receipt and checkpoint in the separate HoldingRead database before acknowledging the write-side lease. The TS claimant uses the same Inventory-owned predicate as an exclusion. An unavailable native worker leaves these events pending; it never enables a legacy fallback.

The fresh candidate browser journey uses normal Account sign-in and SSO, actual Finance/warehouse/product setup, Service create, generic Supply edit, generic Flow default, submit, and independent approval/post. Lost successful create and edit responses are retried with identical keys, payloads and captured context. Independent SQL reconciles stock quantity and value, balanced journal, people, receipt/audit/outbox uniqueness, scoped Flow default and Flow event/fence cardinality. This is Count capability evidence, not a native Service reservation/issue/billing journey or complete Field acceptance. Original broader TypeScript interoperability assertions remain explicitly historical.

Post-removal qualification includes targeted native and retained regressions, reference/dependency checks, actual browser continuation and worker drain, typecheck, lint, generated contracts, catalogue, architecture, documentation and repository structure on the candidate. Hosted quota-exhausted jobs are not presented as passed; the maintainer authorized local verification instead.

Docker follows the qualified Rust capability. A shared minimal non-root Rust runtime serves API and worker entrypoints; it does not package the removed TypeScript Count backend or create one container per owner. Other retained owner services and Next rendering remain declared external dependencies during transition. The current leased-development CLI is not a production configuration interface. Container build/run, network configuration, image size and runtime evidence are separate gates, not implied by this document or by the native tests.

Native generic detail history joins each audit event to its matching receipt, actor, assignment, selected Company and Count/Flow record type. This intentionally corrects the retained generic query's record-ID-only collision exposure. Real PostgreSQL tests compare complete legitimate responses before and after cancellation, with actual lot balances as well as untracked Counts, and verify collision exclusion separately. History orders by creation time; equal-time rows have no imposed tie order.

The actual post-retirement browser read rendered the posted Count in Supply's existing drawer, with 3 units and THB 9.00 adjustment; native generic detail and Flow GET returned the matching identity/status/version. The native CLI drained all eight observed Count/Flow events into a separately migrated leased HoldingRead database, with exact receipt/ack set equality. Candidate processes and all four leased databases were cleaned afterward. These observations remain bounded Count evidence; no complete 206-workflow entry or owner count is promoted.
