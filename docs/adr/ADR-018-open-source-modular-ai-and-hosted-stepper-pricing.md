# ADR-018: Open-source modular AI and hosted stepper seat pricing

> **Next.js era: Reconfirmed (2026-09-02) — still authoritative**
> See `CUTOVER.md` (2026-09-02) and `ADR-020-nextjs-stack.md`.


**Status:** Accepted for implementation
**Date:** 2026-08-28

## Context

ERPX has 21 workspaces (`crates/delivery-contract/src/lib.rs:57` `Workspace::ALL:21`) and 31 bounded contexts (`Cargo.toml` members). Local dev is PASS (`audits/CURRENT-CERTIFICATION.md:22`) but production is NOT YET CERTIFIED (`audits/CURRENT-CERTIFICATION.md:48`). Business goal shifted from single startup vertical to **open-source, sellable to any business via sub-modules**, with AI integrated everywhere as `suggestion / autofill / recommend` per-field, not just chat.

Previous AI was centralized: `agent-orchestration` pool/governance + `Ai` workspace chat (`ADR-017` phases 0-6). That charges for chat and hides AI from customers who buy only `Finance` or `POS`. Cost control via 100/month quota was rejected — customer pays own LLM API directly via `Custom URL + OpenAI Compatible`, so we must charge for **capability, not usage**.

Requirement: all 21 modules must be usable for free (open source) enough to run a real company. Paid only for advanced cross-department Flow AI.

## Decision

### 1. All modules free, AI per-module free

Every workspace (`Finance, Sales, Procurement, Inventory, Asset, Maintenance, Quality, Marketing, People, POS, Legal, Manufacturing, Planning, Project, Commerce, Talent, Learning, Benefits, FieldService, Operations, Support, Ai`) plus `Organization/Identity/Brand` is open source with CRUD + workflow + human approval (`ADR-017:118` human-only guards remain).

Per-module AI is free and unlimited when customer brings own key: rule + small-model autofill from history, 3 suggestions, field anomaly warning, in-module chat. No quota. Provider config is `OpenAI Compatible` with `Custom URL` — `OpenAI` is default preset. See `crates/ai-provider`.

### 2. Paid is cross-department Flow intelligence

`Flow` = durable cross-context fact chain, e.g. `lead -> quote -> order -> requisition -> purchase-order -> receipt -> vendor-invoice -> payment` (`ADR-017:176` procurement-to-payment). Paid AI analyzes the whole Flow, not a single field, and proposes cross-module actions with `proposed -> human apply` (`ADR-017:106`).

Paid unlock is **capability-gated, not usage-gated**. Customer still pays LLM API directly (or Topup via us). We charge **Support subscription OR API Topup** — customer chooses.

### 3. Provider model

- `AiProviderConfig` stored per Holding (free: 1 set/Holding; paid: per-module override). Fields: `endpoint_url` (must be `https://`, validated like `crates/console-bff/src/lib.rs:26` `validate_runtime_origins`), `api_key` (encrypted via `aes-gcm` like `crates/identity`), `model` (free-form, OpenAI compatible), `provider` enum (`OpenAi | Custom` free, plus `Anthropic/Gemini/Azure/Ollama` paid).
- `Test Connection` validates `GET /models` or `POST /chat/completions` with probe.
- Preset list: `OpenAI` primary, `Custom URL` always available.

### 4. Hosted stepper seat pricing

Self-host remains free forever. Hosted SaaS steps per active seat (active `PrincipalId` with login in billing cycle):

| Seats in Holding | Hosted price per seat |
|---|---|
| 1-5 | $9 |
| 6-20 | $7 |
| 21-50 | $5 |
| 51-200 | $3.5 |
| 200+ | Custom |

Blended stepper: 10 seats on Starter = 5*$9 + 5*$7 = $80/mo. Example 30 seats on Growth = 20*$7 + 10*$5 = $190/mo. Hosted includes DB/MinIO/worker hosting, backup/PITR (`cargo xtask pitr rehearsal`), and `verify-web-production` evidence.

AI Flow add-on: **$29 / Holding / mo flat** (unlimited seats/flows) OR **Topup $20 = $20 LLM credit + 10% fee** as alternative to own key. No quota on free AI — free AI unlimited via own key.

### 5. Delivery

- New crate `crates/ai-provider` (neutral, no SQLx/Tonic/Dioxus) — see `crates/ai-provider/src/lib.rs`.
- `crates/delivery-contract` adds `AiProviderConfig`, `AiProviderKind`, `HostedPlan`, `SeatStep`, `FlowAnalysisRequest/Response` and `ConsoleGateway` methods.
- `crates/contexts/commercial` adds stepper pricing domain (`hosted_pricing.rs`) + entitlement flag `flow_ai`.
- Console BFF `apps/console/src/server.rs:26` style env plus per-Holding DB config UI under `Settings > AI Provider`.
- `Dioxus` fullstack remains edge facade (`apps/console` `server` feature with `dioxus/fullstack` `Cargo.toml:13`), not collapsing `apps/server` + `apps/worker` (`ADR-014`).

## Consequences

- Community can run any business type fully free with AI per-module — adoption maximised.
- Revenue from Flow intelligence (capability) and hosted seat steps — not from locking modules or counting LLM calls.
- No LLM cost burden on us when customer brings own key; Topup is optional pass-through.
- Exact Department/Team `Subtree` (`crates/kernel/src/lib.rs:130`) and human-decision proofs (`ADR-017:318`) remain mandatory for Flow AI.

## Alternatives rejected

- Per-seat flat price (unfair to small holdings) — stepper is fair.
- Quota on free AI (against requirement, customer pays own API).
- Central `Ai` workspace chat only — rejected, AI must be per-field everywhere.