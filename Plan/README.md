# ERPX — Open-Source Business Plans

> **Code is truth.** Every capability below traces to implemented code (`crates/proto`, `crates/domain`, `crates/contexts`, `crates/migration`, `apps/console`, `apps/server`). Archived baselines: `Plan.legacy-2026-08-29/` (aspirational FSDs) and `Plan.legacy-codebase-2026-08-29/` (technical snapshot).

## What ERPX is

Web-first, Rust/Dioxus modular monolith ERP. Dioxus console on `:3004`, Tonic gRPC on `:50051`, HTTP auth/health on `:50052`. DDD + Clean + Hexagonal + CQRS. Single deployable that scales from local dev to self-hosted to hosted. See `01-vision-and-scope.md`.

## Who it's for

Everyone — open-source. Self-hosters, contributors, enterprise evaluators, and operators. No hidden commercial gate: the code in this repo is the product.

## How to use this Plan

- `01-vision-and-scope.md` — vision, scope, licensing, deployment tiers, contribution model, and the **universal `@`-mention** contract.
- `02-capabilities/<group>/<context>.md` — 31 lightweight one-pagers (Q5=A, <80 lines each) grouped by value stream (Q8=B). Each links to proto + domain + data + verification.
- `CONTEXT.md` (repo root) — code-derived glossary, no implementation.
- `Plan/glossary.md` — pointer to `CONTEXT.md`.
- `Plan/traceability.md` — RPC/route/table → code → spec matrix.

## Quick links

- Verify (full gate): `cargo xtask verify`
- Fast: `cargo xtask check-affected` / `cargo xtask test-affected`
- Single harness: `cargo xtask test <harness> [filter]`
- ADRs: `adr/` (18 decisions, authoritative for isolation/security/interop)

## Edit rule (Q18=A)

PRs welcome. Any `Plan/` edit must update `traceability.md` and reference the code SHA it describes. Docs = code.
