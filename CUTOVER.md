# ERPX Development Cutover Record

This record does not assert production readiness or external vendor certification.

## 2026-09-02 Rust → Next.js — hard cut to docs-only + Next.js rebuild

Cutover completed on 2026-09-02 on branch `development`: the entire Rust/Dioxus/Tonic/SQLx
monorepo (137 crates, 1321 `*.rs`, 37 `*.proto`, 28 SQLx migrations, 8 `apps/*`) was removed
and replaced by a fresh **Next.js 16 App Router** scaffold (`src/app/`, React 19, Tailwind 4, TypeScript).
The Rust code was not migrated — docs were preserved.

- **Commits:** archive point `f97b74d6` (`backup/rust-2026-09-02`), deletion `d15909bb`, scaffold `29136c81`, legacy prune `99a66de7`.
- **Tags/baseline:** `rust-archive-2026-09-02` at `f97b74d6`, pre-cutover baseline `cc0887dd3f16fe8dbdc827c1655750d9fafd91d2`.
- **Archived then pruned on this branch (A,B):** `crates/proto/proto/erpx/**` (37 proto + descriptors) and `crates/migration/migrations*` (28 SQL files) were copied to `docs/contracts/proto/` + `docs/schema/migrations*` then pruned from `development` — reference remains on `backup/rust-2026-09-02`. `Plan.legacy-2026-08-29/` (43 md) and `Plan.legacy-codebase-2026-08-29/` (40 md) were also pruned.
- **Recovery:**
  ```bash
  git checkout backup/rust-2026-09-02
  git show rust-archive-2026-09-02 --stat   # f97b74d6
  git show cc0887d --stat
  ```
- **Next.js verification on this branch:** `npm run lint` + `npm run build` (replaces `cargo xtask verify` which remains on backup).

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


## Finance-first production monthly close (2026-08-30)

Finance is the first **production-real** vertical (Holding/Group, international-only). V1 scope closes a real month end-to-end with audit trail and hard period gate; it is the verifier for parallel cutover.

**Scope realized:**
- Company-scoped Chart → balanced double-entry Journals → AP/AR subledgers (hybrid Procurement/Sales-owned + finance-direct) → CSV bank import → match proposal/decision → reconciliation → Trial Balance → P&L/BS → AR/AP aging → hard `BeginPeriodClose` → `ClosePeriod(decision)` → `ReopenPeriod(decision)` gated by `HumanDecisionProof` (single-human, separation-of-duties).
- Multi-currency required: spot `FxRate` (micros 1_000_000, MIN 1 MAX 1e12, overflow-checked) with `base_minor` + unrealized FX revaluation via `RevalueOpenSubledgers(period_id, FxSnapshot)`.
- Banking v1 is **CSV-only** (`SaveBankMatchProposal` system → `SaveBankMatchDecision(human)` → `SaveBankReconciliation(expected_version)` idempotent); provider live jobs remain flagged `FINANCE_BANK_PROVIDER_ENABLED=false` and disabled behind `erpx_control.finance_bank_provider_import_jobs`.
- International-only: `StatutoryTaxFact` retained but `GetStatutoryTaxReport` returns empty/generic; TH VAT 7%/WHT/e-Tax deferred.
- Migration toolkit is the "full migration": `ImportChartCsv` + `ImportOpeningTrialBalance` → single balancing opening `Journal` (period 0 `opening`); verifier checks `SUM(debit)==SUM(credit)` and `imported TB == legacy TB` hash, idempotent via `CommandId`/`idempotency_key`, locked after verification. Not full history replay.
- Deferrals locked: budget vs actual, fixed-asset depreciation, inventory costing/variance, intercompany eliminations.

**Cutover gate — parallel + TB-match (not big-bang):**
- Legacy remains authoritative for 1–2 closes; ERPX ingests verified opening TB + journals.
- Gate flips only after `TB(legacy) == TB(erpx)` + `aging(legacy) == aging(erpx)` for the closing date, plus `cargo xtask verify` green.
- Evidence: `ImportChartCsv` (50 accounts) + 20 mixed-currency journals + hybrid vendor/customer invoices + 100 tx CSV bank import → match → reconcile → TB/BS/PL/aging → close/reopen → TB-match report under `audits/` (now `docs/archive/dioxus-2026-08-evidence/` 22 dirs + `backup/rust-2026-09-02:audits` 330 dirs).

**Contracts added (wire-compatible, 26→31 RPCs):** `RevalueSubledgers`, `ImportChartCsv`, `ImportOpeningTrialBalance`, `GetFinancialStatements`, `ListAging` (see `crates/proto/proto/erpx/finance/service.proto` and `Plan/traceability.md`).

**Console:** `/finance` Overview (KPI/TB health) · Chart · Journals · Payables/Receivables · Bank Import CSV→Match→Reconcile · Periods Close/Reopen (proof dialog) · Reports TB/BS/PL/Aging; typed `@` chips + `ReferenceQueryService.search` + `FinanceShellStatus` Fresh/Stale/Offline/Denied.

**Ops:** `FINANCE_BANK_PROVIDER_ENABLED=false` in v1; `fx_rates`/`reval_journal_link` in holding data pair; no new Topo tables beyond fx/reval link. `ops/` living Next.js runbooks (`ops/runbooks/nextjs-incident.md`, `nextjs-recovery.md`, `ops/README.md`) replace Dioxus `docs/archive/ops-dioxus-2026-09-02/`; `Plan/README.md` edit rule Q18=A still applies.

## Verification

`cargo xtask verify` passed after the cutover. It serially ran:

- `cargo fmt --all --check`;
- workspace clippy with all targets, offline, and warnings denied;
- the complete non-ignored workspace test suite;
- all locally runnable ignored SQLx tests against fresh disposable databases,
  including identity, tenant isolation, provisioning, G13 import and
  reconciliation, outbox/projector convergence, Finance candidate activation,
  Operations, support, MFA, and five-minute revocation paths;
- Dioxus web, desktop, host-mobile, iOS, and Android checks, plus host and
  Android bridge checks;
- a real Dioxus web build;
- protobuf descriptor validation and the repository legacy-runtime scan; and
- live `/health`, `/ready`, and port 3004 smoke checks.

The only excluded ignored gates require infrastructure not configured in this
development profile: credentialed HTTPS MinIO backup/restore, isolated
WAL-archive PITR, and credentialed Generic LDAP/AD DS/UCS certification
environments. Their exclusion is explicit and no success is synthesized.

The active runtime is ERPX: Tonic on `127.0.0.1:50051`, auth/health HTTP on
`127.0.0.1:50052`, and the Dioxus console on `0.0.0.0:3004`. Protected HTTP
routes reject unauthenticated access with ERPX identity semantics, and no Node
or Folio process serves port 3004.
