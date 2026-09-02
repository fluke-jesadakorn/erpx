# 01 — Architecture (Codebase-Derived)

**References:** `Cargo.toml` (workspace), `crates/kernel`, `crates/domain`, `crates/contexts/*`, `crates/adapters/*`, `crates/proto`, `apps/server`, `apps/worker`, `apps/console`, `adr/*`, `crates/migration/migrations/`.

## 1.1 Style

- **DDD** — bounded-context ownership, aggregates, invariants live in `crates/domain` and `crates/contexts/<ctx>/src/domain.rs`. Cross-context contracts are versioned protobuf in `crates/proto`.
- **Clean Architecture** — dependencies point inward: kernel → domain → application → adapters → apps. Domain imports no framework/transport/provider types.
- **Hexagonal** — ports in `crates/contexts/<ctx>/src/ports.rs`; adapters in `crates/adapters/<ctx>-sqlx`, `crates/infrastructure`, `apps/*`.
- **CQRS** — normalized authoritative writes + outbox `erpx_control.outbox_events` → independently rebuildable read projections with checkpoints (`projection_checkpoints`, `projection_event_receipts`). No long-term dual writes (ADR-003).

## 1.2 Runtime topology

| Process | Crate | Bind |
|---|---|---|
| Console (Dioxus Web) | `apps/console` | `127.0.0.1:3004` |
| Tonic gRPC | `apps/server` | `127.0.0.1:50051` |
| HTTP auth/health | `apps/server` | `127.0.0.1:50052` |
| Worker | `apps/worker` | outbox/projection/heartbeat consumers |
| Migrator | `apps/migrator` | `sqlx migrate` |
| WAL archive | `apps/wal-archive` | PITR spool |
| Data-plane agent | `apps/data-plane-agent` | outbound customer-side |

Host-native deps: PostgreSQL, MinIO, Ollama. Local creds via `.env.local`; `.env`/`.runtime/.sqlx/target` git-ignored. Reconfigure via `cargo xtask reconfigure-local`.

## 1.3 Workspace decomposition

- **Apps:** `bootstrap`, `console`, `server`, `worker`, `migrator`, `wal-archive`, `data-plane-agent`, `ucs-lab`.
- **Contexts (22+):** `agent-orchestration`, `assets`, `benefits`, `brand`, `collaboration`, `commercial`, `crm`, `customer-360`, `customer-authority`, `ecommerce`, `field-service`, `finance`, `identity-access`, `inventory`, `learning`, `legal`, `maintenance`, `manufacturing`, `marketing`, `operations`, `organization`, `people-payroll`, `planning`, `platform-administration`, `pos`, `procurement`, `projects`, `quality`, `sales`, `support`, `talent`.
- **Shared:** `kernel` (ids, `AuthorityScope`, `DataClassification`), `identity`, `application`, `infrastructure`, `cqrs`, `web-session`, `ui-*`, `console-*`.
- **Contracts:** `crates/proto/proto/erpx/*/service.proto` (+ common/interface/catalog).

## 1.4 Data & isolation

- Two logical DB namespaces: `erpx_control_*` and `erpx_tenant_*`, each with write/read pair. ADR-013 modes: `holding_pair` (all data via holding pair) or `holding_and_branch_pairs` (holding/company via holding pair, branch via branch pair). Branch provisioning: reserve keys → provision DBs → copy aggregates/outbox/projections → fence delta → digest/watermark verify → atomic route activation.
- Directory authority: holding has ≤1 active connector; branch has ≤1 active view referencing holding connector; preflight verifies provider, forest fingerprint, naming context, TLS/CA trust, identity namespace. Username/email are hints only.

## 1.5 Frontend

- Dioxus 0.8 Web-first (ADR-010/016). Console routes defined in `apps/console/src/routes.rs` (`AppRoute`). Workspaces: `/operations`, `/finance`, `/sales`, `/procurement`, `/inventory`, `/asset`, `/maintenance`, `/quality`, `/marketing`, `/people`, `/pos`, `/legal`, `/manufacturing`, `/planning`, `/project`, `/commerce`, `/talent`, `/learning`, `/benefits`, `/field-service`, `/ai` + platform/identity/reference. Hot reload via `cargo xtask dev --detach`.

## 1.6 Verification

Canonical gates in `README.md` at repo root. `cargo xtask verify` serializes fmt → clippy (warnings denied) → workspace tests → SQLx disposable-DB acceptance → Dioxus checks + Web build → descriptor/legacy-scan → live smoke. `verify-platforms` is manual/deferred.
