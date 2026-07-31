# G1-004-QA-UNIVERSAL-BOUNDARY

## Audit status

Read-only audit of the current ERPX boundary snapshot on 2026-07-24. No source,
migration, runtime, database, planning, or Folio files were changed. The only
write permitted by this packet is this evidence report.

The proto currently declares nine typed RPCs in
`crates/proto/proto/erpx/platform/v1/platform.proto:134-152`. The server installs
one Tonic interceptor at `apps/server/src/main.rs:1969-1974`, and every typed
handler is additionally wrapped by `grpc_handler!` at
`apps/server/src/main.rs:63-68`.

## Executive findings

1. **Highest-risk bypass — high: `RenewRole` does not use the authoritative
   commercial/SoD mutation policy.** It authenticates the session, checks the
   role permission, actor, and non-expanding target scope at
   `apps/server/src/main.rs:1027-1059`, then calls the role lifecycle service at
   `:1064-1075`. Unlike the support mutations, it never calls
   `AppState::authorize_entitled_command` (`:209-315`), so no subscription ID,
   billing state, module entitlement, or SoD decision is required at this
   boundary. Domain renewal still enforces finite windows, parent containment,
   and high-risk approval (`crates/domain/src/access.rs:438-475`), but that is
   not a substitute for the missing commercial/SoD gate.

2. **High — correlation integrity is split across header names.** The handler
   wrapper chooses `x-erpx-correlation-id` or `x-correlation-id`
   (`apps/server/src/boundary.rs:41-52`), while common context validation checks
   only `x-erpx-correlation` (`:292-300`). A request can therefore carry a
   correlation in the request context/audit and a different correlation in the
   error metadata without being rejected. HTTP independently reads only
   `x-erpx-correlation` (`apps/server/src/main.rs:1469-1475`).

3. **High — worker boundaries have no request/event tracing context or
   structured/redacted error contract.** `WorkerError` is only
   `Retryable(String) | Permanent(String)`
   (`crates/infrastructure/src/worker.rs:10-16`); outbox retry/dead-letter paths
   persist those strings directly (`:119-174`), and projection errors are
   converted to `PersistenceError::InvalidData(error.to_string())`
   (`:272-305`). The Stripe semantic worker similarly persists parser/application
   strings (`apps/worker/src/lib.rs:424-480`). Outbox envelopes contain
   correlation and actor fields (`crates/infrastructure/src/outbox.rs:30-52`),
   but current handlers do not propagate them into spans or structured errors.

4. **Medium — HTTP stable code classification has a concrete 5xx bug.**
   `errors::http_error_code` maps every `500..=599` status to `unavailable`
   (`apps/server/src/errors.rs:165-176`), including internal server errors.
   HTTP responses are otherwise normalized and correlated by
   `write_http_response` (`apps/server/src/main.rs:1821-1835`).

5. **Medium — identity authentication error classes are inconsistent.** The
   identity HTTP `authenticate` helper maps every token authorization failure,
   including a possible database failure, to `401`
   (`apps/server/src/identity_http.rs:309-321`), while the direct login/refresh
   mapper returns database failures as `503` and invalid records as `400`
   (`apps/server/src/main.rs:1422-1431`). The identity route mapper returns an
   invalid record as `500` (`apps/server/src/identity_http.rs:333-359`).

6. **Medium — readiness is configuration-level, not dependency-level.**
   `/readyz` checks only the control pool, identity readiness, and storage
   configuration (`apps/server/src/main.rs:1498-1527`). Storage explicitly
   reports `live_connectivity: "not_probed"`
   (`apps/server/src/storage.rs:168-193`); worker, projection/read databases,
   commercial processing, tenant routing, and live object-store connectivity
   are not part of readiness.

The current ledger agrees that no G1-004 gate is claimed and specifically leaves
worker/tenant/commercial readiness, provider/raw-body retention, complete worker
ErrorContext propagation, role-renewal commercial/SoD policy, and universal
mutation adoption open (`EXECUTION_LEDGER.md:738-740`, `:954-956`).

## Typed RPC handler matrix

`grpc_handler!` supplies a correlation ID and enriches every returned Tonic
`Status` (`apps/server/src/main.rs:63-68`). The interceptor also verifies the
access token and transport actor/session attribution
(`apps/server/src/main.rs:1339-1373`). The matrix below records the
handler-specific checks observed after that common layer.

| RPC / evidence | Actor + session | Scope/resource binding | RBAC | Billing / entitlement / SoD | Support capability | Error/correlation result | Finding |
|---|---|---|---|---|---|---|---|
| `ListCustomers`, `main.rs:600-665` | `authorize`, query context | Platform scope only; global list | `platform.customer.read` at `:614-622` | Not applied; control-plane platform read | N/A for platform read | Application errors at `:631`, wrapper at `:604` | No observed customer-specific bypass; each returned customer is globally audited at `:637-660`. |
| `GetCustomer`, `main.rs:668-733` | `authorize`, query context | Platform scope; customer ID inspected at `:691-699` | `platform.customer.read` at `:682-690` | Not applied; control-plane platform read | N/A for platform read | Application errors at `:699`, wrapper at `:672` | Global-read audit records actor/session, customer, purpose, scope, and correlation at `:700-728`. |
| `CreateTicket`, `main.rs:744-787` | `authorize`; command actor checked by boundary and helper | Customer binding and command scope through `authorize_entitled_command` at `:762-770` | Explicit ticket role `:751-759`; helper role check `main.rs:231-238` | Helper checks billing, entitlement, effective role, SoD, and mutation state at `main.rs:239-296` | Explicitly disabled in helper (`support_capability_required: false`, `:290-293`); reasonable for customer-originated intake, but the exception is implicit | Application adapter at `:776`; wrapper at `:748` | No direct bypass observed; add a policy-exception test so this cannot silently become a support-agent mutation path. |
| `RequestSupportSession`, `main.rs:789-857` | `authorize`, actor match `:805-810` | Canonical approved scope containment `:811-822`; customer passed to helper | `support.session.request` at `:796-804` | Helper checks customer binding, billing, entitlement, role, SoD at `:826-834` and `main.rs:217-296` | N/A while requesting approval; capability is created only after approval | Application adapter `:852`; wrapper at `:793` | Request-stage checks present. Customer/ticket relationship remains an application/persistence invariant and should have a direct negative test. |
| `ApproveSupportSession`, `main.rs:859-927` | `authorize`, actor match `:864-869` | Loaded session customer is used by helper `:901-917` | Customer approver and support approval roles at `:871-887` | Helper checks billing, entitlement, effective role, SoD at `:909-917`; customer approval token verified at `:918` | N/A for customer approval control | Application adapter `:924`; wrapper at `:863` | Approval boundary is present. Two-person semantics are represented by the customer approver role plus support request/approval flow; no independent worker audit was observed. |
| `RevokeSupportSession`, `main.rs:929-975` | `authorize`, actor match `:934-949` | Session is loaded and its customer bound at `:950-966` | `support.session.revoke` at `:936-944` | Helper checks billing, entitlement, role, SoD at `:958-966` | N/A for emergency/control revocation | Application adapter `:971`; wrapper at `:933` | Enforcement is present; the success log at `:972` omits correlation/request ID and should be structured through the boundary context. |
| `ListExpiringRoles`, `main.rs:985-1020` | `authorize`, query context | Query tenant scope passed to `list_expiring_in_scope` at `:997-1014` | `role.assignment.read` at `:998-1006` | Not applied; control-plane role read | N/A | Application adapter `:1015`; wrapper at `:989` | Read-only/control-plane exception. No global-read disclosure audit is recorded in this handler. |
| **`RenewRole`**, `main.rs:1022-1080` | `authorize`, grantor match `:1027-1044` | Requested scope must contain current assignment scope at `:1029-1059` | `role.assignment.renew` at `:1030-1038` | **Missing: no `authorize_entitled_command`; `CommandContext.subscription_id` is not required; no billing/entitlement/SoD lookup** | N/A for control-plane role lifecycle, unless a support actor is later allowed to call it | Application adapter `:1075`; wrapper at `:1026` | **Highest-risk mutation gap.** Domain checks parent and high-risk approval, but the server policy intersection is incomplete. |
| `CheckEntitlement`, `main.rs:1092-1290` | `authorize`, query context | Customer binding at `:1123-1132` | Effective assignment at `:1209-1224` | Billing `:1187-1197`, entitlement `:1198-1207`, SoD `:1225-1235`, effective access and module decision `:1237-1273` | Capability ID/scope/device/session/ticket/secret checks at `:1135-1183` when supplied | Wrapper at `:1096` | Query decision path covers the universal inputs; it is not itself a mutation and does not record a decision audit. |

### Common policy observations

`AuthContext::authorize` performs write-side token/session authorization and
matches claims to `x-erpx-actor` and `x-erpx-session`
(`apps/server/src/main.rs:124-154`). `authorize_entitled_command` then checks
actor identity, persisted customer scope, role, required subscription, billing,
module entitlement, effective role chain, SoD, and mutation access
(`apps/server/src/main.rs:217-296`). Its explicit
`support_capability_required: false` setting means the helper cannot enforce a
capability unless the caller supplies a separate support-aware path; current
support request/approval calls are control-flow stages, while revoke is a
support-capability control operation rather than a tenant business mutation.

The domain itself does implement important role invariants: finite expiry and
strict `valid_until > valid_from` (`crates/domain/src/access.rs:355-372`), the
half-open active interval `valid_from <= at < valid_until` (`:375-386`), parent
scope/duration/authority/permission containment (`:388-417`), high-risk
approver separation (`:419-436`), and immutable renewal using a new UUID and
incremented version (`:450-475`). Those checks do not close the server-level
commercial/SoD omission in `RenewRole`.

## Error, correlation, and HTTP matrix

| Boundary | Observed behavior | Gap / risk |
|---|---|---|
| Tonic transport interceptor | Installed for all four services; verifies access token and actor/session; calls `errors::enrich` on interceptor failures (`main.rs:1969-1988`, `:1339-1373`). | Correlation ID is selected from alternate headers before canonical validation, creating split audit/error correlation (`boundary.rs:41-52`, `:292-300`). |
| Tonic typed handlers | All nine methods use the wrapper; `errors::enrich` attaches `x-erpx-error-code`, `x-erpx-retryable`, `x-erpx-correlation-id`, and JSON `x-erpx-error-context` (`errors.rs:43-63`). | `ErrorContext.details` is always empty (`errors.rs:33-40`) and no protobuf `Status::details` payload is emitted. Application authorization/conflict/assignment/support reason strings can remain public because only internal/unavailable/deadline messages are forced to fallback (`errors.rs:65-82`, `:84-105`). |
| Application-to-Tonic mapping | Repository errors become static `unavailable`; not-found/conflict and domain errors get stable status classes (`errors.rs:84-105`). | `Assignment` and `Support` `to_string()` values are not centrally classified/redacted; future dynamic provider/business text can cross the boundary. |
| HTTP response normalization | All normal responses pass `write_http_response` and receive the stable JSON envelope plus correlation/security headers (`main.rs:1821-1835`; `errors.rs:127-163`). | `500..=599` all map to `unavailable` (`errors.rs:165-176`). The response trace records route, correlation and latency only, without status/outcome (`errors.rs:179-209`). Early request-parser failures happen before the trace is created (`main.rs:1469-1475`, `:1838-1885`). |
| HTTP identity | Identity routes intentionally omit credential material (`identity_http.rs:63-65`); login/refresh/revoke enforce token/session behavior in `main.rs:1565-1694`. | `authenticate` collapses all token-store errors to 401 (`identity_http.rs:309-321`); invalid-record mapping differs between `identity_http.rs:347-349` and `main.rs:1422-1431`. No correlation-aware identity audit event was observed in these handlers. |
| `/healthz`, `/readyz`, `/metrics` | GET-only methods and response correlation/security headers are present (`main.rs:1488-1564`, `:1821-1835`). | Readiness checks control, identity, and storage configuration only; storage says live connectivity is not probed (`main.rs:1498-1527`, `storage.rs:168-193`). |
| Stripe webhook | Signature is verified over the raw body with a five-minute tolerance, JSON is parsed, and event inbox insert is idempotent (`main.rs:1739-1817`, `crates/infrastructure/src/commercial.rs:756-787`). Invalid signature/JSON/header is 400; configuration/persistence is 503 (`main.rs:1534-1558`). | Raw-body digest is computed then discarded (`main.rs:1469-1470`); no webhook audit/correlation retention is visible at this boundary. Inbox `last_error` is only length-bounded to 4096 characters, not redacted (`commercial.rs:905-995`, `:1178-1180`). |

## Worker and internal boundary matrix

These are system-worker operations rather than end-user RPCs, so actor/session/
RBAC/entitlement/support checks are not expected at the worker loop itself.
They still require a trusted system actor/event context, tenant/resource binding,
correlation propagation, and redacted structured failure handling.

| Worker path | Observed checks | Missing / risk |
|---|---|---|
| Executable composition | Only `customer_360` and `stripe_semantic` handlers can be selected (`apps/worker/src/main.rs:9-52`). | No worker identity/span initialization beyond `tracing_subscriber::fmt::init()` (`:5-8`); no correlation or audit context is attached to a pass/event. |
| Generic outbox loop | Claims owned events, invokes handler, and performs retry/dead-letter transitions (`crates/infrastructure/src/worker.rs:119-174`). | `WorkerError` is unstructured strings (`:10-16`); event metadata is not passed into an error context/span. |
| Customer 360 projection | Checks projection and aggregate type, parses payload, writes read model and bindings (`apps/worker/src/lib.rs:288-327`). | Trusts envelope payload and scope without an explicit actor/tenant/resource consistency check; database failures become raw strings. |
| Projection rebuild/switch | Replacement is prepared, rebuilt, then activated (`apps/worker/src/lib.rs:240-263`; infrastructure replay `worker.rs:248-314`). | Switch/rebuild failures lose structured context; `replay_dead_letter` is a callable store operation with no visible authenticated platform command boundary (`apps/worker/src/lib.rs:265-273`). |
| Stripe semantic worker | Claims inbox events, dispatches typed semantic events, marks processed, retries or dead-letters (`apps/worker/src/lib.rs:407-484`; dispatcher `crates/infrastructure/src/commercial.rs:198-216`). | No event correlation/audit span; parser/application errors are persisted as caller-provided strings (`apps/worker/src/lib.rs:444-480`). The raw body is deliberately not returned by the claim path, but error text is not centrally redacted. |

## Prioritized implementation packets

The following packets are intentionally non-overlapping and should be integrated
in this order:

1. **G1-004-ERROR-CANONICALIZATION (main/AG-FND):** choose one canonical
   correlation header, reject conflicting aliases, and make every interceptor,
   handler, HTTP response, audit record, and error context use it. Fix HTTP 5xx
   classification and unify identity-store error mapping. Add boundary tests for
   alias conflicts, 500/503, database-auth failures, and correlation equality.

2. **G1-004-ERROR-CONTEXT-WIRE (main/AG-FND):** define the compatibility form of
   `ErrorContext.details` for Tonic (metadata versus `Status::details`), classify
   application errors centrally, and redact dynamic assignment/support/provider
   text. Add tests that assert no connection strings, tokens, secrets, or raw
   provider payloads cross gRPC/HTTP or appear in persisted failure text.

3. **G2-ROLE-RENEWAL-POLICY (AG-IAM + main integration):** route `RenewRole`
   through the authoritative mutation policy or add a dedicated control-plane
   policy that explicitly evaluates identity/session, role scope, billing state,
   entitlement, and SoD. Require/validate subscription metadata where the role
   lifecycle is customer-scoped. Test absent subscription, past-due/grace,
   read-only billing, missing entitlement, failed SoD, and allowed control-plane
   renewals.

4. **G1-004-WORKER-CONTEXT (AG-FND):** introduce a bounded system-worker/event
   context carrying event ID, correlation, causation, tenant scope, aggregate,
   and worker identity; use it in outbox/projection/Stripe spans, structured
   retry/DLQ errors, and replay authorization. Add tenant mismatch, correlation
   preservation, redaction, retry, and replay authorization tests.

5. **G1-004-READINESS (AG-FND/AG-PRV):** add explicit worker, projection/read
   database, commercial inbox, tenant router, and live object-store probes with
   redacted status. Test each dependency failing independently and verify
   `/readyz` cannot report ready.

6. **AG-QA boundary regression wave:** rerun the handler-by-handler matrix and
   the direct tests above after packets 1–5; do not infer universal mutation
   coverage from the shared helper until every mutation has an explicit policy
   classification.

## Direct test recommendations

- Enumerate the nine typed RPCs and assert malformed context, missing session,
  actor mismatch, conflicting correlation headers, and enriched error metadata
  on each.
- Invoke `RenewRole` under missing subscription, inactive entitlement, past-due
  and read-only billing, and failed SoD; assert rejection before the application
  command.
- Assert role domain boundaries at `valid_from`, exactly `valid_until`, parent
  expiry, sibling scope, permission widening, permanent expiry, high-risk
  self-approval, and immutable renewal.
- Assert `500` produces `internal` while `503` produces `unavailable`; assert
  identity database failures are 503 on every identity route and invalid-record
  classification is consistent.
- Assert worker event correlation/tenant scope survives successful projection,
  retry, dead-letter, replay, and Stripe semantic processing; assert sensitive
  error text is redacted.
- Make readiness dependency-specific and assert a configured-but-unreachable
  MinIO/read/worker/commercial dependency yields 503.

## Verification commands and exact results

Commands were run from `/Users/fluke/Desktop/Work/folio/erpx`.

| Command | Result |
|---|---|
| `cargo test -p erpx-server -- --nocapture` (initial parallel focused run) | PASS: 23 passed, 0 failed, 0 ignored. |
| `cargo test -p erpx-worker --lib -- --nocapture` (initial parallel focused run) | PASS: 1 passed, 0 failed, 1 ignored; ignored test requires a disposable PostgreSQL control database. |
| `cargo test -p erpx-infrastructure --lib worker::tests -- --nocapture` (initial parallel focused run) | PASS: 1 passed, 0 failed, 16 filtered out. |
| `cargo test -p erpx-infrastructure --lib commercial::tests -- --nocapture` (current rebuild) | **FAIL before tests ran**: `crates/infrastructure/src/lib.rs:19-20` imports `ProviderCatalogPrice` that is not in `commercial` and imports `CatalogReconciliationReport`/`ProviderCatalogRevision` privately; errors E0432/E0603. |
| `cargo test -p erpx-server -- --nocapture` (current rebuild) | **FAIL before tests ran** on the same E0432/E0603 errors, plus warnings for unused imports in `crates/infrastructure/src/repositories.rs:6,12-16`. |
| `cargo test -p erpx-worker --lib -- --nocapture` (current rebuild) | **FAIL before tests ran** on the same infrastructure re-export errors and `crates/infrastructure/src/repositories.rs:68` missing `customer_projection_from_payload` (E0425), plus unused-import warnings. |
| `cargo test -p erpx-infrastructure --lib worker::tests -- --nocapture` (current rebuild) | **FAIL before tests ran** on the same infrastructure re-export/function errors. |

The initial PASS results were obtained before the later current rebuild exposed
the live workspace compile errors. The rerun failures are the authoritative
current verification state for this audit snapshot. No ignored or disposable
database test was run, and no runtime or database mutation was performed.

## Security concerns and false-completion risks

- A successful `cargo test` result from an earlier compiled artifact must not be
  treated as current verification while the workspace currently fails to
  compile through infrastructure.
- The existing shared commercial helper can create an impression of universal
  mutation coverage even though `RenewRole` bypasses it. Every mutation needs an
  explicit N/A or enforced-policy classification.
- Domain role invariants are strong evidence for aggregate behavior, not proof
  of authenticated server authorization, commercial authority, or SoD.
- Error metadata presence is not proof of safe error content: dynamic
  failed-precondition/permission messages and worker/DLQ strings still need a
  redaction test.
- A configured object store and healthy control database do not prove live
  tenant/read/worker/commercial availability.
- Worker replay and projection APIs are internal seams in the current code; they
  must not be exposed through a future platform RPC without an authenticated,
  audited operator boundary.

## Rollback

This audit made no source, migration, database, runtime, or Folio changes.
Rollback is limited to removing this report:

`erpx/audits/G1-004-QA-UNIVERSAL-BOUNDARY/evidence/README.md`
