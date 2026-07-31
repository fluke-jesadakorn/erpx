# Coordinator continuation — bounded completion hardening — 2026-07-27

This packet records the local hardening completed after the prior G0–G14
verification wave. It is evidence, not a gate-closure claim.

## Delivered slices

- G2: recovery retries rehydrate and validate the deterministic request-ID
  assignment; the server binds the durable review receipt to the committed
  assignment's authoritative `valid_from`.
- G7: Operations persisted aggregates and source events reject nil hierarchy
  IDs and non-concrete Platform/Subtree scopes.
- G8: AP/AR allocations require exact party and currency identity.
- G9: SCM recosting rejects missing authoritative average cost without
  mutating stock state.
- G10: invalid consent records cannot authorize Marketing delivery.
- G11: completed/refunded POS sales require full payment; failed HR/payroll
  review/post transitions and version overflow leave state unchanged.
- G12: shared navigation sanitizes every target through deep-link redaction.
- G13: evidence writes are fenced by the locked migration-run version.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-application --offline` — PASS: 19 tests.
- `cargo test -p erpx-domain --offline` — PASS: 229 tests plus Finance/POS
  integration tests.
- `cargo clippy -p erpx-domain --all-targets --offline -- -D warnings` — PASS.
- `cargo test -p erpx-ui --offline` — PASS: 79 tests plus UI integration
  tests.
- `cargo test -p erpx-server --bin erpx-server --offline` — PASS: 84 active,
  one ignored disposable database test.
- `cargo test -p erpx-infrastructure g13_migration::tests --lib --offline` —
  PASS: 2 tests; `cargo test -p erpx-migration g13::tests --lib --offline` —
  PASS: 3 tests.
- Folio `bun run lint` and `bunx tsc --noEmit` remain green; the four lint
  warnings are pre-existing `infra/scripts/start-n8n.js` warnings.

## Boundaries and rollback

The slices use authoritative domain validation and existing SQLx/outbox/read
model seams; no provider credentials, mocks, schema-only completion, live
vendor certification, or production state was introduced. Revert only the
listed product/application files and this packet to roll back this local
continuation. No directory activation, command-owner switch, Folio write
retirement, or cutover occurred.

Open evidence remains: full serialized workspace verification after this
continuation, ignored database targets, external Generic LDAP/AD DS/UCS
certification, measured one-million-user/five-minute runs, native mobile
secure storage and device execution, approved protobuf descriptor input, live
Folio migration/rollback/retention/post-cutover proof, and explicit production
approval.

## Final local verification update

The final serialized workspace verification is complete: format, warnings-
denied workspace Clippy, non-ignored workspace tests/doc tests, all Dioxus
targets, protobuf/G13/G14 focused acceptance, Folio lint/typecheck, the
read-only structural audit, and the port-3004 probe pass. The fresh serial
ignored database run passed every local target. Its only failure was the
credentialless G14 external-readiness preflight; the exact disposable
databases were removed. The Finance handoff acceptance also passes after its
fixture now binds payment and invoice to the same authoritative party.

The remaining items are certification and approval evidence, not local test
failures: credentialed LDAP/AD/UCS environments, measured million-user and
five-minute external runs, physical mobile execution, an approved descriptor
baseline, live Folio migration/rollback/retention/post-cutover evidence, and
explicit production approval. No activation, command-owner switch, Folio
write retirement, or cutover occurred.

## Current-tree hardening addendum — directory and delivery boundaries — 2026-07-27

- Directory authentication now derives MFA/authentication evidence from the
  authentication boundary rather than a caller timestamp, rechecks local
  break-glass grants at verification time, and redacts MFA references from
  debug output. Directory feeds reject empty or repeating LDAP, AD DirSync,
  RFC 4533, and UCS continuation positions; cursor progression remains
  provider-specific and fail-closed.
- Directory RBAC rejects contradictory role facts, immutable-link rebinds,
  same-revision mapping conflicts, and delimiter-collision identifiers while
  preserving append-only revocation facts. The combined identity, feed,
  SQLx-acceptance, sync, and volume suites pass; identity Clippy with warnings
  denied passes.
- Marketing provider delivery now uses the database clock for claim-result
  evidence, renews a live claim before recording a result, bounds provider
  submission, and keeps the trusted internal delivery helper crate-private.
  Worker and infrastructure focused tests passed; the fresh disposable live
  provider-flow acceptance passed and removed its database afterward.
- The external-readiness acceptance now requires exact secret-free references
  for Generic LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning Service
  inputs, including the five-minute revocation evidence; the credentialless
  preflight remains intentionally queued.

These changes are local evidence advances and do not constitute vendor
certification, million-user measurement, production activation, or cutover.

## Current-tree follow-up — G3–G6, G9–G11, and evidence quality — 2026-07-27

- G3–G6 platform control now derives holding-scoped authorization from a
  stable holding key; per-holding connectors cannot collapse to platform
  scope. Platform domain and infrastructure focused tests and warnings-denied
  domain/infrastructure Clippy pass.
- G9–G11 payroll validation rejects empty, malformed, duplicate, or invalid
  employee lines; SCM transfers require exact scoped product/warehouse
  references and validate both balances before mutation. Domain tests pass
  with the new regressions.
- Directory SQLx acceptance tests requiring PostgreSQL are explicitly ignored
  unless `ERPX_G14_LOCAL_DIRECTORY_DATABASE_URL` is configured. The ordinary
  test records the exact queued blocker; the coordinator's ignored suite must
  provide a disposable URL for genuine persistence evidence.
- The current worker scheduler composition and Marketing callback ingress are
  present in source and focused tests; older QA-062 “missing composition” and
  “missing callback” statements are historical findings superseded by the
  current tree. Their ignored live SQLx acceptance remains queued until a
  disposable database run is executed.
- Folio `tsconfig.json` excludes generated `erpx/target` artifacts so the
  required `bunx tsc --noEmit` remains reproducible after Dioxus checks.

These changes still do not provide vendor credentials, million-user or
five-minute external measurements, physical mobile execution, an approved
protobuf baseline, live Folio cutover evidence, or production approval.

## Final coordinator verification addendum — 2026-07-27

- PASS: `cargo fmt --all --check`; workspace Clippy with warnings denied after
  the final worker-scope correction; and the clean `cargo test --workspace
  --offline --no-fail-fast` run, including the 84-test identity suite, 239-test
  domain suite, 197 worker tests, and all doc tests.
- PASS: the fresh ignored workspace run with disposable control, tenant,
  commercial, scheduler, support, and directory databases. Directory SQLx
  import/RBAC/revocation persistence, all local G3–G13/G14 database targets,
  and Marketing callback/provider-flow persistence executed successfully.
  Disposable databases were removed. The sole nonzero target was the
  intentionally ignored credentialless G14 external-certification preflight;
  it listed the exact Generic LDAP, AD DS/DirSync, and UCS references and made
  no certification claim.
- PASS: G13 deployment/protobuf surfaces (4/4), G13 parallel acceptance
  (8/8), G14 directory acceptance (8/8), and G14 external-readiness checks
  (7 pass, 1 explicitly ignored certification preflight). The descriptor
  comparison remains queued because no approved baseline is configured.
- PASS: Dioxus web, desktop, host-mobile, iOS, and Android checks; Folio
  `bun run lint` and `bunx tsc --noEmit`; the read-only structural audit; and
  the port-3004 `/operations` probe (HTTP 307). Lint has four pre-existing
  warnings in `infra/scripts/start-n8n.js` and zero errors.

G0–G14 are not represented as 100% closed: credentialed Generic LDAP/RFC
4533, AD DS/DirSync, and UCS Provisioning Service certification; measured
one-million-user imports and five-minute revocation; physical mobile device
execution; an approved protobuf descriptor baseline; live Folio source/target
migration, rollback, retention, and post-cutover evidence; and explicit
production approval remain queued. No activation, command-owner switch,
Folio write retirement, or cutover occurred.

## Current-tree AD DirSync protocol correction — 2026-07-27

- The live AD DS response-control decoder now follows the authoritative BER
  shape `SEQUENCE { moreData BOOLEAN, unused INTEGER, cookie OCTET STRING }`.
  The previous local decoder incorrectly required the `moreData` field to be an
  INTEGER and would reject real AD DirSync responses before SQLx application.
- Focused worker transport tests pass for boolean true/false responses,
  malformed trailing bytes, TLS endpoint/fingerprint binding, and connector
  revalidation. This is local protocol evidence only; credentialed AD DS
  certification, measured import/revocation evidence, and vendor acceptance
  remain queued.

## Coordinator verification addendum — 2026-07-27

- After the BER correction, formatting, the offline workspace tests, and
  workspace Clippy with warnings denied passed; focused worker directory-live
  tests passed 8/8.
- Protobuf checking, G13/G14 focused acceptance, all five Dioxus target checks,
  Folio lint/typecheck, the read-only structural audit, and the port-3004 route
  probe passed. No production Next.js build was used.
- A fresh socket-qualified ignored database run passed every local database
  target. The only expected nonzero result was the credentialless G14 external
  certification preflight; all 13 disposable databases were removed.
- Credentialed LDAP/AD/UCS certification, measured external scale/revocation
  evidence, the approved protobuf descriptor baseline, live Folio
  migration/cutover/rollback/retention evidence, physical mobile evidence, and
  explicit production approval remain queued.

## Parallel completion continuation — local hardening — 2026-07-27

- Directory authentication now requires validated loopback peer-origin
  evidence for local break-glass requests and revalidates that origin at
  authentication time; non-loopback IPv4/IPv6 rejection tests pass.
- Directory full-import conversion preserves disabled/deleted identities,
  rejects immutable-envelope mismatches, and live LDAP/RFC4533/AD paths fail
  closed when their native continuation control is missing or duplicated.
- G7 Operations reconciliation now rejects orphan tenant outbox rows that have
  no authoritative Operations event. G8 journal posting now locks and compares
  the persisted accounting period before emitting journal, usage, outbox, or
  idempotency rows.
- G12 rendering now treats invalid directory/reconciliation/rollback evidence
  as unavailable or read-only. G13 persistence acceptance now proves expired
  lease reclaim, stale-claim rejection, retry exhaustion, and projected DLQ
  state against fresh PostgreSQL databases.
- G13/G14 runtime references, vendor endpoint/server-name bindings, evidence
  paths, and unsafe LDAP/AD query/path forms now fail closed. Missing external
  credentials and production references remain queued without activation.
- Additional local closures include connector-scope reload/locking for directory
  RBAC, source-fenced Finance banking rebuilds, exact HR outbox continuity,
  holding-bound platform read-model projection, role-review notification-state
  validation, and Operations orphan-outbox detection. Real MinIO round-trip
certification remains queued because the local MinIO endpoint and credentials
are unavailable.

## Final serialized verification record — 2026-07-27

- PASS: `cargo fmt --all --check`, workspace Clippy with warnings denied, and
  the non-ignored workspace tests/doc-tests.
- PASS: a fresh ignored SQLx run across directory identity, G3–G13/G14,
  Finance, HR, Legal, SCM, POS, Marketing, Operations, provisioning, support,
  and G13. Every local target passed. Its only nonzero target was the
  intentionally ignored G14 external-certification preflight, which failed
  closed for missing credentialed Generic LDAP/AD DS/UCS references. Eighteen
  disposable databases created for this run were explicitly removed.
- PASS: G13 deployment 4/4, G13 parallel 9/9, G14 directory 8/8, G14
  readiness 7/7 plus one intentionally ignored preflight, protobuf
  compilation, all five Dioxus checks, Folio lint/typecheck, the structural
  probe, and the port-3004 `/operations` probe (HTTP 307).

The approved protobuf descriptor baseline, credentialed vendor certification,
measured one-million-user/five-minute external evidence, physical mobile
execution, live Folio migration/cutover/rollback/retention evidence, MinIO
round-trip, and explicit production approval remain queued. No activation,
command-owner switch, Folio write retirement, or cutover occurred.

## G2 runtime continuation — role-review snapshot and notification worker

- Added a production worker handler that reads authoritative role assignments,
  evaluates a bounded domain review queue for one explicit scope, and persists
  the review snapshot plus notification rows through SQLx.
- Registered the existing notification worker in the executable and supplied a
  real control-plane outbox publisher. Publication is idempotent by the stable
  notification key and the claim is reconciled only after the event append.
- Focused role-review worker tests: PASS (7/7). Fresh disposable PostgreSQL
  role-review acceptance: PASS (2/2 ignored tests), including outbox replay
  idempotency and recovery receipt replay.
- This is local G2 evidence only. It does not claim external notification
  provider delivery; activation/cutover and production ownership remain
  approval-gated.

## Native UCS continuity correction

- UCS Provisioning Service now requires exact sequence continuity after the
  first observed message. Regressions and gaps fail closed; exact duplicates
  remain in the durable already-committed confirmation path.
- Focused worker directory-live tests: PASS (12/12). Credentialed UCS vendor
  acceptance and five-minute revocation measurement remain queued.

## Final corrected verification record — 2026-07-27

- PASS: formatting, warnings-denied workspace Clippy, serialized workspace
  tests/doc-tests, protobuf compilation, G13 recovery/parallel acceptance,
  G14 directory/readiness acceptance, all five Dioxus checks, Folio lint and
  TypeScript, structural audit, diff hygiene, and the port-3004 HTTP 307
  probe.
- PASS: fresh serial ignored SQLx evidence for every local database target;
  the local directory SQLx target was explicitly rerun with its disposable
  URL and passed 2/2. The only intentional nonzero target is the ignored
  credentialless G14 vendor preflight, which records exact LDAP/AD DS/UCS
  blockers and claims no certification. Eighteen verification databases were
  removed after the run.
- QUEUED: credentialed Generic LDAP/RFC4533, AD DS/DirSync, and UCS
  Provisioning Service acceptance; measured one-million-user imports and
  five-minute revocation; approved descriptor comparison; physical mobile
  execution; live Folio migration/rollback/retention/post-cutover proof;
  MinIO round-trip; and explicit production approval. No activation, owner
  switch, Folio write retirement, or cutover occurred.
