# ERPX Docs — Single Hub (TOC)

> **One hub for everything.** Start at `README.md`, land here, then follow the walk that fits you. This file replaces the prototype in `.scratch/docs-seamless/prototype/TOC-seamless.md`.

## Tree — 3 layers (after 2026-09-02 prune)

```
README.md                           # entry — Next.js reboot 2026-09-02, recovery: backup/rust-2026-09-02 / tag rust-archive-2026-09-02 / cc0887d
├── AGENTS.md + CLAUDE.md           # @AGENTS.md — tooling (Node 20+ / npm) + skills; fences at top/bottom are machine-owned
├── CUTOVER.md                      # 2 entries: 2026-09-02 Rust→Next.js (new, on top) + 2026-07-31 Folio→Rust (history)
├── CONTEXT.md                      # canonical glossary (46 terms) — implementation-free, single source for every term
├── CONTRACT_POLICY.md              # Next.js policy: REST/tRPC + Prisma/Drizzle (protobuf era archived on backup)
├── Plan/                           # business plan — 31 contexts, living spec
│   ├── README.md                   # Code is truth → archived contracts on backup + living src/app
│   ├── 01-vision-and-scope.md      # vision, tiers (Local → Self-hosted → Hosted), @-mention contract
│   ├── 02-capabilities/<group>/<context>.md  # 13 groups / 31 contexts — lightweight one-pagers
│   ├── 03-cross-cutting/@-and-ai/  # 33 deep specs per context (AI, @-mentions, field patches)
│   ├── traceability.md             # Archived Rust RPC table (banner) — living Next.js routes TBD
│   ├── glossary.md                 # 5-line pointer → CONTEXT.md
│   └── LEGACY.md                   # stub → backup for pruned baselines (Plan.legacy-* removed 2026-09-02)
├── adr/                            # 20 ADRs: 15 Reconfirmed ✅ + 3 Deprecated (010,014,016) + 1 Superseded (002→020) + 020 Next.js stack
├── docs/
│   ├── TOC.md                      # ← you are here
│   ├── README.md                   # archive status — contracts/schema pruned on this branch, see backup
│   ├── contracts/README.md         # inventory of archived 37 proto files + descriptors (pruned, on backup)
│   ├── schema/README.md            # inventory of archived 28 SQL migrations (pruned, on backup)
│   ├── contracts/next/             # (future) OpenAPI / tRPC route table — empty until src/app routes exist
│   ├── archive/delivery-2026-08/   # Delivery/README.md 768L (Rust DDD) — archived with banner
│   ├── archive/wayfinder-2026-09-02/ # wayfinder decisions (map + 10 tickets + prototype) — audit trail
│   └── agents/                     # skills meta — domain.md (single-context) + issue-tracker.md (GitHub)
├── Designed/
│   ├── README.md                   # stub — 27 PNGs + routes /work-queue, /billing/* + link to Plan
│   ├── AGENTS.md                   # Pencil-only guardrail
│   └── MainDesign.pen (15M) + images/ (27 PNG, 16M)  # single source — do not grep .pen
├── audits/                         # after prune 330→~40, 72M→<20M (see audits/README.md)
│   ├── README.md                   # index — before/after + keep-list (~40 dirs)
│   ├── CURRENT-CERTIFICATION.md    # Dioxus cert (archived) + Next.js gate pointer
│   └── (keep) DIOXUS-CUTOVER*, ADR-017-PHASES-0-6, e2e-all-modules, finance_darkmode_fixed, finance_live_full …
├── fixtures/ + ops/                # fixtures (ai/* 4 json, migration) + runbooks (ops/runbooks/* Deprecated)
├── src/app/                        # living Next.js — layout.tsx, page.tsx, globals.css, future route handlers
├── .agents/skills/                 # canonical 6 skills (domain-modeling, grilling, prototype, research, wayfinder)
├── .github/workflows/
│   ├── nextjs.yml                  # living CI — npm run lint + build
│   └── erpx-targets.yml            # Deprecated — Rust CI, kept for history
└── .gitignore                      # Next.js + legacy Rust ignores
```

## How to walk the docs (3 paths)

1. **Newcomer:** `README.md` → `Plan/01-vision-and-scope.md` → pick one `Plan/02-capabilities/<group>/<context>.md` you care about → `CONTEXT.md` for terms → `src/app/page.tsx` (code) → if you need history: `CUTOVER.md` + `Plan/LEGACY.md` + `docs/README.md` → `git checkout backup/rust-2026-09-02`.

2. **Find a term:** every `Plan/*/glossary.md` points here → `CONTEXT.md` is the only canonical glossary (46 terms, no implementation).

3. **Find history:** `CUTOVER.md` (2 entries) + `Plan/LEGACY.md` + `docs/README.md` → `backup/rust-2026-09-02` + tag `rust-archive-2026-09-02` + baseline `cc0887d` + `f97b74d6` archive point.

## Links

- Entry: `../README.md`
- Business plan: `../Plan/README.md` → `../Plan/01-vision-and-scope.md` → `../Plan/02-capabilities/`
- Glossary: `../CONTEXT.md`
- Decisions: `../adr/ADR-020-nextjs-stack.md` (living) + `../adr/`
- Archive: `README.md` + `contracts/README.md` + `schema/README.md` + `archive/`
- Design: `../Designed/README.md` (single source) + `../Designed/MainDesign.pen` (Pencil MCP only)
- Evidence: `../audits/README.md` → `../audits/CURRENT-CERTIFICATION.md`
- Code: `../src/app/page.tsx` + `../src/app/layout.tsx`
- Cutover: `../CUTOVER.md`

## Notes

- No file is duplicated in two places anymore (legacy dirs deleted, `audits/` pruned, contracts split archive/living, skills single-homed at `.agents/skills/`).
- The 7 critical contradictions (second cutover invisible, protobuf vs pruned, 558 dead links) are resolved by banners in `CUTOVER.md`, `CONTRACT_POLICY.md`, `Plan/traceability.md`, and this hub.
- If you came from `.scratch/docs-seamless/prototype/TOC-seamless.md` — that was the throwaway prototype; this `docs/TOC.md` is the living one.
