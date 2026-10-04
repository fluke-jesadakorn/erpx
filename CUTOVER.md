# ERPX Cutover Record

## 2026-09-27 Canonical attachment lifecycle

[ADR-036](docs/adr/ADR-036-canonical-file-sources-and-policy-bound-links.md) replaces owner-bound upload reads with immutable private File Sources, purpose-scoped File Sets and Links, quarantine scanning, audited access, and separately approved sanitized Media Publications. Holding-write migrations `0331`–`0335` and control migration `0020` are additive. Historical bytes are never presumed clean: the global attachment gate stays closed until backfill, malware scanning, ownership classification, checksum/storage/quota reconciliation, and the final upload-freeze delta all report zero blocking backlog. Rollback disables new reserve and bind operations while retaining clean bound evidence, links, and append-only audit history; it never downgrades or deletes evidence. The legacy upload and source-ID routes remain compatibility adapters for one release and must resolve an authorized canonical link.

## 2026-09-15 IAM policy and identity expansion

[ADR-025](docs/adr/ADR-025-multiple-identity-providers-and-scoped-replacement.md) adds simultaneous optional AD, Entra ID and UCS connections, reviewed Company-scoped identity replacement and immutable custom role revisions. Control 0006/0007 and Holding-write 0020/0021 are additive; existing business data, audit attribution and assignment expiry dates are preserved. Built-in permission revisions invalidate existing selected contexts, requiring users to reselect a work role. Legacy invitations and access writes now require independent review and fresh MFA. Unversioned queued human-authorized jobs must be explicitly retried. Directory configuration does not activate migration; pilot batches require separate review and emergency readiness. See the [IAM operations contract](docs/contracts/next/identity-access.md).

## 2026-09-06 Three-level workspace URL hard cut

The opaque-ID browser/API namespace was removed without redirects. Canonical immutable-slug routes now express Platform, Holding, Branch, Holding Department, and Branch Department workspaces. Company remains an internal selected or lineage-derived policy boundary. ADR-023 supersedes the public-routing clause of ADR-001; opaque Holding IDs remain authoritative for bindings, storage, queues, and audit.

## 2026-09-02 Rust → Next.js (hard cut)

Rust/Dioxus/Tonic/SQLx 137 crates 1321 rs 37 proto 28 SQL 8 apps removed and replaced by Next.js 16 App Router (`src/app/`, React19, Tailwind4, TS). Docs preserved, code not migrated.

- Archive `f97b74d6` (`backup/rust-2026-09-02`), del `d15909bb`, scaffold `29136c81`, prune `99a66de7` (37 proto + 28 SQL briefly copied to `docs/` then pruned; `Plan.legacy-*` 43+40 pruned — see the recovery references below)
- Tags: `rust-archive-2026-09-02` @`f97b74d6`, baseline `cc0887d`
- Recovery: `git checkout backup/rust-2026-09-02; git show rust-archive-2026-09-02 --stat; git show cc0887d --stat`
- Gate on this branch: `bun run lint`, `bun run typecheck`, `bun run test:integration` and isolated `bun run deploy:build` (replaces `cargo xtask verify` on backup)

## 2026-07-31 Folio → Rust — bounded development reset

Cutover completed on 2026-07-31 in the local development environment.

## Recovery boundary

- Final pre-cutover source baseline:
  `cc0887dd3f16fe8dbdc827c1655750d9fafd91d2`
  (`checkpoint: preserve pre-ERPX cutover bridge state`).
- The former Next.js/Bun application, Node tooling, bridge source and worker,
  bridge configuration, active historical documentation, Folio database files,
  and Folio-only service templates were deleted from the active tree.
- Existing ERPX implementation and acceptance evidence was promoted to the
  root `audits/` directory (now archived at `docs/archive/dioxus-2026-08-evidence/` 22 dirs 3.9M on `development`; 330 dirs 72M on `backup/rust-2026-09-02`).
- Source recovery is available from the baseline commit. Folio database and
  object data were intentionally deleted without archival; there is no data
  rollback.

## Bounded development reset

- Dropped only these schemas from local `folio_db`: `ai`, `auth`, `chat`,
  `finance`, `folio`, `hook`, `hr`, `inventory`, `law`, `perm`, and `platform`.
- Preserved the `public` schema and all host-native services (the former `n8n` schema was retired by ADR-015).
- Deleted only the local `folio-storage` MinIO bucket and created the
  `erpx-demo` namespace.
- Removed `.runtime/n8n/` per ADR-015. The former encryption configuration SHA-256 was
  `bfe114069c47d741f8e8e2de89dfe857bc6f9064bf2a1e31e348eb2215656cf6`
  at retirement.
- Created separate `erpx_control_write`, `erpx_control_read`,
  `erpx_tenant_write`, and `erpx_tenant_read` databases.
- `cargo xtask reset-dev` is loopback-only, schema/bucket allow-listed,
  target-printing, and gated by the literal `--confirm RESET_ERPX_DEV`.

## ERPX authority and fixture evidence

- Persisted command ownership is `erpx_only`, established by
  `cutover:migration-0066`.
- Immutable ownership evidence ID:
  `4e1a4e86-f821-5f73-84e7-04f30bcc055c`.
- Append-only outbox event:
  `erpx.command_ownership.established.v1`.
- Idempotent demo state contains 22 product contexts, one seed receipt,
  22 deterministic context outbox events, four native identities, and four
  finite role assignments. Secrets are hash-only.
- Directory connector count is zero. LDAP/RFC4533, AD DS/DirSync, and UCS code
  remains compiled, while development readiness is explicitly `disabled`;
  activation fails closed and is audited.
- Approved protobuf descriptor SHA-256:
  `a5838f78ee5828d444fb7743f6732e43a92a7b553cbb94cea9afe45f5a70e82d`.


## Finance-first monthly close (2026-08-30) — summary

Finance is first production-real vertical (Holding/Group, international-only, `docs/archive/dioxus-2026-08-evidence/finance-e2e-2026-08-30.md` + `finance_live_full/`): Chart→Journals→AP/AR→CSV bank→match→reconcile→TB/P&L/BS/aging→`BeginPeriodClose`→`ClosePeriod`/`ReopenPeriod` via `HumanDecisionProof`; multi-currency `FxRate`+`RevalueOpenSubledgers`; CSV-only banking (`FINANCE_BANK_PROVIDER_ENABLED=false`); 26→31 RPCs (`RevalueSubledgers` `ImportChartCsv` `ImportOpeningTrialBalance` `GetFinancialStatements` `ListAging`); gate `TB(legacy)==TB(erpx)` + `cargo xtask verify` green before cutover (legacy 1–2 closes authoritative). Console `/finance` + `ops/` Next.js runbooks replace Dioxus.

## Verification (archived)

`cargo xtask verify` passed: fmt/clippy/workspace tests/SQLx acceptance/Dioxus web+desktop+mobile/build/descriptor/health+ready `:3004/:50051/:50052` (see backup `docs/archive/dioxus-2026-08-evidence/` 22 dirs).


## 2026-10-03 Documentation consolidation and recovery

This cleanup changes documentation and inventory projection, not domain interfaces, migrations or environment deployment. Surviving ownership/workflow controls from the removed Rust maps and September plans are consolidated in [the generated catalogue](docs/contracts/next/catalogue.md) and the current owner contracts. The dated engineering receipt retains its original source/candidate; cleanup is not a new whole-catalogue verification.

Verified Git recovery points: current pre-cleanup HEAD `3fa064af7767debbb8a2a3f6085ced610f6286c5`; Rust archive tag `rust-archive-2026-09-02` resolves to `f97b74d63c11273a4ef241017411412bd797264a`; baseline `cc0887d` exists. Inspect historical files with `git show <commit>:<path>` or an isolated checkout. Never restore old contracts/runtime over a working development tree. Changes that were not present in Git were copied to ignored `.scratch/dev/documentation-cleanup/before/` before removal.


## 2026-10-04 Unused legacy code cleanup

Removed six unused People/Technology components and their five scoped stylesheets, the old Cloudflare provisioning/migration/restore scripts and inactive D1 test, the one-time SQLite baseline generator, and unused OpenNext/Workers beta/jobs configuration. The unused `@opennextjs/cloudflare` and `@tiptap/extension-image` dependencies were also removed. Current People and Technology workspaces, public routes/APIs, business data and PostgreSQL migrations are unchanged.

The historical `local-review` E2E adapter still uses Wrangler; its main configuration, generated types and SQLite migration inputs remain. Runtime import adapters, canonical attachment compatibility and old browser move-tray state remain in use. UI inventories were regenerated; only entries for deleted files were removed from the frozen-boundary register, preserving every retained fingerprint.

All removed files were verified recoverable from pre-cleanup HEAD `3fa064af7767debbb8a2a3f6085ced610f6286c5` with `git show <commit>:<path>`. Their current working-tree contents, including uncommitted changes, plus the original full diff and edit targets were saved under ignored `.scratch/dev/legacy-cleanup-20261004T144250Z/` before removal. Use that local snapshot to recover uncommitted versions; do not restore historical runtime files over the active workspace. This cleanup builds and verifies the development source without installing a release or changing staging.

Validation: lint (zero errors, five existing warnings), typecheck, documentation checks and the isolated release build passed. Integration reported 2,320 passing tests, five failures and three skips; all five failures were reproduced against the saved pre-cleanup source and dependency snapshot. The UI guard has 13 pre-existing failures (14 before cleanup), with no new failure and all 107 retained frozen fingerprints unchanged. The current People/Technology workspaces and image insertion in the Home editor were checked through the authenticated development console; the image test was closed without saving or publishing. Development and private production health checks returned HTTP 200; the development server stays on port 3100.
