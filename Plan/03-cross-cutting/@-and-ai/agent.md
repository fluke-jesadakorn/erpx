# Agent — Deep `@` + AI (Production)

**Group:** `13-ai-collaboration` · **Code:** `crates/contexts/agent` · **Proto:** `crates/proto/proto/erpx/agent/service.proto` (`AgentOrchestrationCommandService, AgentOrchestrationQueryService` · 30 RPCs) · **Adapters:** `crates/adapters/agent-sqlx` · **UI:** `apps/console/src/features/agent` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `Plan/02-capabilities/13-ai-collaboration/agent.md`. Central polymorphic `@` contract in `Plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (30)

- **Services:** AgentOrchestrationCommandService, AgentOrchestrationQueryService
- **RPCs:** ** RegisterAgent, AddAgentVersion, ApproveAgentVersion, SuspendAgent, RetireAgent, StartAssignment, DelegateAssignment, RetryAssignment, CancelAssignment, RegisterPool, ReplacePoolMembers, RoutePool, StartPrivateSession, AppendPrivateSessionMessage, PublishPrivateSessionMessage, SetPrivateSessionLegalHold, ExpirePrivateSession, RequestFieldSuggestion, StartFieldSuggestion, CompleteFieldSuggestion, ApplyFieldSuggestion, RejectFieldSuggestion, GetAgentDefinition, GetAssignment, GetPool, GetPrivateSession, GetRoutingDecision, GetInvocationEvidence, GetFieldSuggestion, ListFieldSuggestionsByRecord

| RPC | Service | Data touch |
|-----|---------|------------|
| `** RegisterAgent` | Command/Query | `agent` aggregate + outbox |
| `AddAgentVersion` | Command/Query | `agent` aggregate + outbox |
| `ApproveAgentVersion` | Command/Query | `agent` aggregate + outbox |
| `SuspendAgent` | Command/Query | `agent` aggregate + outbox |
| `RetireAgent` | Command/Query | `agent` aggregate + outbox |
| `StartAssignment` | Command/Query | `agent` aggregate + outbox |
| `DelegateAssignment` | Command/Query | `agent` aggregate + outbox |
| `RetryAssignment` | Command/Query | `agent` aggregate + outbox |
| `CancelAssignment` | Command/Query | `agent` aggregate + outbox |
| `RegisterPool` | Command/Query | `agent` aggregate + outbox |
| `ReplacePoolMembers` | Command/Query | `agent` aggregate + outbox |
| `RoutePool` | Command/Query | `agent` aggregate + outbox |

## Invariants (domain — keep)

- Agent versions require approval; assignments are routable and auditable with invocation evidence.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `agent` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `agent definitions/assignments/pools/sessions + field suggestions` (light `agent.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** RegisterAgent → Add/Suspend/Approve version → Start/Delegate assignment → harness — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/ai + /work` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title AGENT, list agent-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/agent/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/agent/src/domain.rs`
- **Adapters:** `agent-sqlx`
- **UI:** `apps/console/src/features/agent`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `agent`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `Plan/MAP.md` for flat index.
