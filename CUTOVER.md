# ERPX Cutover Record

## 2026-09-06 Three-level workspace URL hard cut

The opaque-ID browser/API namespace was removed without redirects. Canonical immutable-slug routes now express Platform, Holding, Branch, Holding Department, and Branch Department workspaces. Company remains an internal selected or lineage-derived policy boundary. ADR-023 supersedes the public-routing clause of ADR-001; opaque Holding IDs remain authoritative for bindings, storage, queues, and audit.

## 2026-09-02 Rust → Next.js (hard cut)

Rust/Dioxus/Tonic/SQLx 137 crates 1321 rs 37 proto 28 SQL 8 apps removed and replaced by Next.js 16 App Router (`src/app/`, React19, Tailwind4, TS). Docs preserved, code not migrated.

- Archive `f97b74d6` (`backup/rust-2026-09-02`), del `d15909bb`, scaffold `29136c81`, prune `99a66de7` (37 proto + 28 SQL briefly copied to `docs/` then pruned; `Plan.legacy-*` 43+40 pruned — see `docs/plan/ARCHIVE.md`)
- Tags: `rust-archive-2026-09-02` @`f97b74d6`, baseline `cc0887d`
- Recovery: `git checkout backup/rust-2026-09-02; git show rust-archive-2026-09-02 --stat; git show cc0887d --stat`
- Gate on this branch: `npm run lint` + `npm run build` (replaces `cargo xtask verify` on backup)

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
