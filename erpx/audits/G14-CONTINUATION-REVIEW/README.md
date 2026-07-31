# G14 continuation review — current G3–G14 gate matrix

**Audit date:** 2026-07-27
**Branch:** `codex/erpx-parallel-completion`
**Owner:** G14 audit consolidation only

This file is the current-tree consolidation of the G3–G14 audit/readme
corpus, the directory packets, and the migrator acceptance tests. It changes
no source, protobuf, server, worker, manifest, migration, plan, ledger, or
runtime file.

## Verdict

**ERPX is not G0–G14 complete.** The latest Sales/SCM/Asset-Maintenance
transport packet and directory UI packet add real source-bound implementation
evidence. They do not establish live SQLx, vendor, device, migration, or
cutover acceptance. No customer directory was activated; Folio remains the
operational write owner; no command-owner switch, Folio write retirement, or
production cutover was performed or authorized.

**Verification refresh:** 2026-07-27. This review records the current green
local non-database wave and keeps unavailable environment evidence explicitly
queued.

## Inspected evidence corpus

The review covered all current G3–G14 audit/readme files (221 files), all four
`DIRECTORY-*` readmes, and these seven current migrator acceptance tests:

- `apps/migrator/tests/g9_product_transport_acceptance.rs`
- `apps/migrator/tests/g11_asset_maintenance_acceptance.rs`
- `apps/migrator/tests/g11_product_context_acceptance.rs`
- `apps/migrator/tests/g11_sales_transport_acceptance.rs`
- `apps/migrator/tests/g13_parallel_acceptance.rs`
- `apps/migrator/tests/g13_deployment_recovery.rs`
- `apps/migrator/tests/g14_directory_acceptance.rs`

Historical packet claims were not promoted over current-tree evidence. Status
uses these meanings:

- **Source-proven:** authoritative code/contracts and a focused executable
  check are present; this is not live acceptance.
- **Partial:** implementation seams exist, but one or more required API/UI,
  worker, reconciliation, rollback, or integration layers remain unproven.
- **Blocked:** the required disposable database, vendor, device, or cutover
  environment is unavailable.

## Current focused evidence

| Check | Result | Boundary |
|---|---|---|
| `cargo test -p erpx-console --offline --all-targets --no-fail-fast` | **PASS — 41 passed, 0 failed** | Console redaction, directory/platform/product read-only surfaces, route authorization, and QA contracts. No authenticated live journey. |
| `cargo test -p erpx-migrator --test g9_product_transport_acceptance -- --nocapture` | **PASS — 3 passed, 0 failed** | Sales, SCM, and Asset/Maintenance protobuf/server/worker transport markers and bounded authorized routing; live SQLx remains queued. |
| `cargo test -p erpx-migrator --test g11_asset_maintenance_acceptance -- --nocapture` | **PASS — 4 passed, 0 failed** | Asset/Maintenance source/read reconciliation, forged-event rejection, and coordinator transport markers; live SQLx remains queued. |
| `cargo test -p erpx-migrator --test g11_product_context_acceptance -- --nocapture` | **PASS — 7 passed, 0 failed** | Source/read/worker evidence and explicit surface-status reporting. Sales, Procurement/Inventory, and Asset/Maintenance bounded read surfaces are now present; HR/Payroll and Legal shell markers remain open, while POS and Marketing/CRM remain partial. |
| `cargo test -p erpx-migrator --test g11_sales_transport_acceptance -- --nocapture` | **PASS — 3 passed, 0 failed** | Sales seven-type replay/read safety and protobuf/server save/read parity; live SQLx and vendor certification remain queued. |
| G14 vendor environment probe | **BLOCKED** — `ERPX_G14_LDAP_ENDPOINT`, `ERPX_G14_AD_DS_ENDPOINT`, and `ERPX_G14_UCS_ENDPOINT` are unset | No LDAP, on-prem AD DS, or UCS certification claim. |
| G14 database reference probe | **BLOCKED** — `ERPX_CONTROL_WRITE_DATABASE_URL`, `ERPX_CONTROL_READ_DATABASE_URL`, `ERPX_TENANT_ADMIN_DATABASE_URL`, and `ERPX_TENANT_DATABASE_URL_TEMPLATE` are unset | No live SQLx, RLS, replay, reconciliation, or rollback claim. |
| `cargo test -p erpx-migrator --test g13_parallel_acceptance -- --nocapture` | **PASS — 5 passed, 0 failed** | Read-only migration/protobuf/restart/idempotency boundary checks; runtime, descriptor, and Folio ownership acceptance remain queued. |
| `cargo test -p erpx-migrator --test g13_deployment_recovery -- --nocapture` | **PASS — 4 passed, 0 failed** | Read-only deployment/recovery and Folio port guard checks; disposable runtime and rollback evidence remain queued. |
| `cargo test -p erpx-migrator --test g14_directory_acceptance -- --nocapture` | **PASS — 4 passed, 0 failed** | Directory contracts, identity-crate public tests, worker registrations, and explicit DB/vendor blocker reporting. No external directory or database connection. |
| Directory administration UI markers | **PASS in checked-in tests** | `DirectoryAdministrationRouteSurface`, authenticated fetch, and read-only `DirectoryAdministrationSurface` are wired in console composition; no live authenticated journey is claimed. |
| `cargo fmt --all --check` | **PASS** | Current tree is formatted. |
| `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** | Warnings-denied workspace Clippy is green. |
| `cargo test --workspace --no-fail-fast` | **PASS** | Current non-ignored workspace tests are green, including application, console, domain, identity, infrastructure, migrator, proto, UI, and worker targets. |
| Dioxus console web/desktop/mobile-host/iOS/Android checks | **PASS** | Cross-target compilation checks are green; no physical device or simulator acceptance is claimed. |
| `cargo check -p erpx-proto --offline` | **PASS** | Local protobuf compilation is green; approved-descriptor compatibility comparison remains unexecuted. |
| `bun run lint` | **PASS — 0 errors, 4 existing warnings** | Warnings are confined to `infra/scripts/start-n8n.js`; no lint failure. |
| `bunx tsc --noEmit` | **PASS** | Folio TypeScript check is green. |
| Folio `http://127.0.0.1:3004/operations` | **PASS — HTTP 307 to `/?login=1&next=%2Foperations`** | Folio development server is listening and unauthenticated access remains protected; no bridge ownership or cutover acceptance. |
| `git diff --check` | **PASS** | No whitespace errors were reported. |

The required ignored database suite was also attempted. Its latest run exited
with 50 failed targets because disposable database references were not
configured; database-independent ignored checks that could run passed. This
is not live SQLx, RLS, replay, reconciliation, rebuild, or rollback evidence.

## G0–G14 gate matrix

| Gate | Implementation evidence currently present | Live DB/vendor/device/cutover blocker | Gate state |
|---|---|---|---|
| **G0 — contracts and fixtures** | Current audit/contract policy, versioned protobuf inputs, migration fixtures, deterministic mapping surfaces, and green local formatting are present. | No approved descriptor baseline or executable compatibility comparison. | **Partial / re-audit required** |
| **G1 — Rust foundation** | SQLx routing, outbox/checkpoint/claim/retry, projector/rebuild, health, metrics, and worker composition seams are present; source-bound tests and warnings-denied Clippy evidence exist. | No disposable control/tenant restart, RLS, replacement-read, DLQ, or replay acceptance. | **Partial / blocked live proof** |
| **G2 — identity and expiring RBAC** | Identity persistence and shared policy primitives exist. Directory tests cover TLS, immutable links, scoped mappings, high-risk MFA, five-minute stale/revoked denial, append-only role facts, and local-only break-glass. | Universal mutation adoption, role-review delivery, end-to-end session/effective-access intersection, and live revocation persistence are unproven; control DB is unavailable. | **Partial / blocked live proof** |
| **G3 — global reads and Customer 360** | Customer 360 domain, scoped read/projection, audit/detail transport, redaction, and platform control seams are present. | Live tenant read routing, rebuild/reconciliation, authenticated deep-link journeys, and complete multi-holding UI acceptance are absent. | **Partial / blocked live proof** |
| **G4 — support and incidents** | Ticket/attachment/SLA/incident persistence, transport, capability and redaction seams, operations completion, and read-only UI contracts are present. | Full intersected authorization, manager/expiry handling, notifications, authenticated workflow journeys, Folio reconciliation, and live tenant execution are unproven. | **Partial / blocked live proof** |
| **G5 — billing and Stripe** | Billing/catalog/entitlement domain, SQLx, webhook/signature/idempotency, provider boundary, and reconciliation packets exist. | No live Stripe credentials/provider execution, catalog/entitlement reconciliation, complete billing command/UI journey, or rollback run. | **Partial / blocked vendor proof** |
| **G6 — provisioning and platform control** | Tenant registry, provisioning, replacement-read, backup/retention, connector, redaction, Platform Operations API, and platform reconciliation read seam are present. | No control/tenant DB, PostgreSQL PITR/restore, MinIO/object, provider, RLS, or candidate activation evidence. | **Partial / blocked live proof** |
| **G7 — operations** | Operations aggregates, persistence/projector, exception lifecycle, reconciliation runtime, worker registration, and read-only console/Folio bridge contracts are present. | Source-context producers, scheduling/detection, expired-principal reassignment, authenticated end-to-end UI, and live reconciliation are unproven. | **Partial / blocked live proof** |
| **G8 — finance** | Finance journal/chart/subledger/banking/close persistence and projection seams, reconciliation logic, candidate activation test, and registered completion worker are present. | No disposable two-database run, bank/provider credentials, AP/AR/close/SoD/consolidation/Thai-statute acceptance, or complete UI workflow. | **Partial / blocked live/provider proof** |
| **G9 — Sales/SCM/Asset transport** | Sales, SCM, and Asset/Maintenance have typed protobuf save/read services, authorized server routing, bounded JSON, write/read database labels, SQLx source/read persistence, outbox/projection/reconciliation/clear seams, and typed worker scheduler branches. The G9 transport verifier passes 3/3. | No live tenant write/read/replay/reconciliation/rollback execution; no vendor/device acceptance. Required G9 test database references are unset. | **Source-proven transport / blocked live proof** |
| **G10 — Marketing/CRM** | Marketing/CRM persistence, consent/content/provider boundaries, typed CRM/Marketing services, read-only console surface, auxiliary handlers, and tenant projection dispatch are present. | No real provider delivery/callback, consent/suppression timing, attribution-to-finance reconciliation, complete journey UI, or live database run. | **Partial / blocked live/provider proof** |
| **G11 — remaining product contexts** | HR/Payroll, Legal/Documents, POS, Sales, SCM, Asset/Maintenance, and Marketing/CRM have current domain/SQLx/read/projector or reconciliation evidence and worker/transport seams. Asset/Maintenance acceptance passes 4/4, Sales transport acceptance passes 3/3, and the product-context verifier passes 7/7; Sales, SCM, and Asset/Maintenance bounded list surfaces are now present, while HR/Payroll and Legal shell markers remain open and POS/Marketing remain partial. | Product workflows, signed object/document storage, payroll/refund/provider execution, durable scheduling, live replay/reconciliation/rollback, vendor evidence, and complete role journeys remain unproven. | **Partial / blocked live proof** |
| **G12 — Dioxus UI and QA** | Product read-only surfaces, safe states/redaction, directory administration model, authenticated directory route/fetch/rendering, product-context coverage, green formatting, workspace tests/Clippy, and web/desktop/mobile-host/iOS/Android checks are present. | No Android/iOS device or simulator journey, secure keystore, notifications, deep links, camera/scanner, offline recovery, or accessibility journey evidence. | **Partial / blocked device proof** |
| **G13 — migration and cutover** | Ordered migration state, read-only Folio extractor/dry-run boundary, deterministic ID mapping, reconciliation values, read-only/cutover guard tests, green local deployment/recovery checks, and Folio port guard are present. | No source/target/rollback DB references, approved descriptor comparison, durable cohort/backfill/final-delta/read model, shadow replay, rollback rehearsal, command-owner switch, Folio write retirement, or retention/retirement evidence. | **Missing / blocked cutover** |
| **G14 — hardening and final audit** | Current directory verifier, worker registration assertions, product transport acceptance checks, UI/read-surface checks, green workspace verification, local hardening tests, and Folio port guard are present. | No requirement-by-requirement live security/reconciliation/restore/rollback/device/vendor acceptance; database and directory environments are unavailable; final cutover remains unauthorized. | **Missing / externally blocked** |

## Directory requirement matrix

| Requirement | Current implementation evidence | Acceptance blocker/classification |
|---|---|---|
| Generic LDAP | TLS policy, attribute mapping, bounded import, RFC 4533 contracts, SQLx staging/checkpoints/members, outbox/read model, and tests. | No credentialed LDAP endpoint; no real handshake, million-user run, restart, or RFC 4533 certification. **Partial / blocked.** |
| On-prem AD DS | DirSync request/response/cursor contracts, objectGUID immutable linking, scoped mapping, SQLx/projector seams, and tests. | No AD DS environment/credential; no rights, cookie restart, disable/delete, ACL, or five-minute timing run. **Partial / blocked.** |
| UCS | HTTPS/TLS policy, Provisioning Service token/sequence contracts, entryUUID linking, SQLx/projector seams, and tests. | No UCS environment/credential; no token restart, replay/order, disable/delete, outage, or reconnect run. **Partial / blocked.** |
| Full import up to 1M users | Bounded `MAX_DIRECTORY_IMPORT_USERS`, staged pages, checkpoints, finalization, duplicate/immutable-link guards, and one-million boundary tests. | No real volume/throughput/memory/restart/reconciliation evidence. **Partial.** |
| Platform plus per-holding connectors | Typed platform/holding scope and server authorization/read-model fields are present. | No live RLS/routed database or multi-holding acceptance. **Partial / blocked.** |
| Directory-managed RBAC and revocation | Role mapping, append-only hash-chained facts, read projection, stale-feed fail-closed policy, five-minute SLA types, MFA policy, and local break-glass policy are present. | No live feed-to-role projection, access measurement, universal authorization, or audit replay. **Partial / blocked.** |
| Native change feeds | RFC 4533, AD DirSync, and UCS Provisioning Service feed contracts are present and provider-bound. | Concrete network adapters and vendor certification are not available; generic LDAP polling remains a separate legacy contract and is not native-feed certification. **Partial / blocked.** |
| TLS-only/MFA/break-glass | Endpoint validation, protocol bounds, high-risk fresh-MFA checks, and local-only fallback tests pass. | No real TLS handshake or end-to-end server authorization/session evidence. **Policy-proven only.** |
| Worker/API/UI | `directory_sync` worker and `DirectoryService` server are registered; the console directory route fetches and renders an authorized read-only model. | No live SQLx transaction/replay/reconciliation/rollback or authenticated external-directory journey. **Partial / blocked.** |

## Required blockers before any green gate or cutover

1. Provide disposable, physically distinct control and tenant write/read
   PostgreSQL environments and run the required ignored SQLx, RLS, replay,
   reconciliation, rebuild, and rollback suites.
2. Provide credentialed generic LDAP/RFC 4533, on-prem AD DS/DirSync, and UCS
   Provisioning Service environments for TLS, full-import, native-feed,
   immutable-link, RBAC, restart, and five-minute revocation certification.
3. Provide the approved protobuf descriptor and execute the compatibility
   comparison. Local formatting and the non-ignored serialized verification
   wave are already green; rerun the full wave after descriptor evidence is
   available.
4. Provide native Android/iOS execution environments for the required device
   journeys and secure-storage/notification/deep-link checks.
5. Complete the five open G11 shell UI markers (Sales, Procurement/Inventory,
   Asset/Maintenance, HR/Payroll, and Legal) and the two partial POS and
   Marketing/CRM surfaces, then run authenticated role journeys.
6. Complete G13 live migration, rollback, ownership, Folio retention, and
   retirement evidence. None of those actions are authorized by this audit.

## Guardrails and rollback

This audit performs no database writes, connector activation, vendor calls,
command-owner change, Folio write retirement, or production cutover. Directory
rollback must preserve immutable identity links, append-only role/revocation
facts, audit history, and outbox events. Read models may be cleared only for
authoritative replay. Any failed source/read reconciliation keeps the surface
stale/read-only and blocks activation.

The only file changed for this consolidation is this README.
