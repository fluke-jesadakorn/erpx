<!-- BEGIN:nextjs-agent-rules -->

# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` (resolved from this file's directory; in monorepos the `next` package may not be visible from the repo root) before writing any code. Heed deprecation notices.

This block is written and re-added by `next dev` — verify at `node_modules/next/dist/server/lib/generate-agent-files.js`. Removing it from a diff only re-creates the uncommitted change; committing it with your work keeps the tree clean.

<!-- END:nextjs-agent-rules -->

# AGENTS.md

ERPX is a Next.js workspace rooted at this directory (rebuilt from Rust/Dioxus on 2026-09-02).

## Tooling

Use Node.js 20+ with npm (or pnpm) and the pinned Next.js toolchain. Rust/Cargo/Dioxus/Tonic/SQLx are no longer part of this branch — full Rust source is archived on `backup/rust-2026-09-02` tag `rust-archive-2026-09-02` and baseline `cc0887d` per `CUTOVER.md`.

```bash
node -v   # 20+
npm -v    # 11+
npm run dev    # Next.js on http://localhost:3000
npm run build  # production build
npm run lint   # eslint
```

## Verification

Keep `npm run dev` hot reload running while editing UI. At a checkpoint:

```bash
npm run lint
npm run build
```

Before handoff, run the full gate once (`lint` + `build` must pass, no TS errors). Browser smoke on `http://localhost:3000` should render `src/app/page.tsx`. Add `playwright` e2e if needed (`npm run build` includes route typecheck).

## Development

```bash
npm install        # install deps
npm run dev        # start dev server (auto-reload on src/app/*, src/components/*, globals.css)
npm run build      # verify production build
```

Environment is kept in `.env` / `.env.local` (ignored). See `.env.example` for required vars. Legacy Rust runtime files (`.runtime/`, `target/`, `.cargo/`) are no longer used — see backup branch for history.

Reload or restart the dev server freely and verify with a live request that `http://localhost:3000` is usable again. The server does not need to remain running at handoff.

## Runtime safety

Keep `.env`, `.env.local`, `node_modules/`, `.next/`, and `out/` outside Git. Do not commit secrets. Past Rust artifacts (`Cargo.toml`, `crates/`, `apps/`, `xtask/`, `third_party/`) were deleted in this cutover — do not reintroduce Cargo tooling on this branch without a new ADR.

## Structure

- `src/app/` — Next.js App Router (layout, page, globals.css, route handlers)
- `public/` — static assets
- `Plan/`, `Plan.legacy-2026-08-29/`, `Plan.legacy-codebase-2026-08-29/` — business plans and traceability
- `adr/` — architecture decision records (18, authoritative for isolation/security)
- `audits/` — retained implementation/acceptance evidence
- `Designed/` — Pencil canvas `MainDesign.pen` (access via Pencil MCP only)
- `docs/` — archived Rust contracts (`contracts/proto/**`) and schemas (`schema/migrations*`) plus `docs/README.md`
- `fixtures/`, `ops/` — deterministic fixtures and runbooks
- `.agents/skills/`, `.claude/skills/` — workflow skills (domain-modeling, grilling, prototype, research, wayfinder)

## Recovery

To view the prior Rust monorepo:

```bash
git checkout backup/rust-2026-09-02
git show rust-archive-2026-09-02 --stat
# or see CUTOVER.md recovery boundary cc0887d
```

<!-- skills:start -->
## barlevalon workflow skills

Workflow skills are installed in `.agents/skills`.
When asked for a named workflow, read that skill's `SKILL.md` before acting.
If the skill links helper files, read those too.

Installed skills:
- `domain-modeling`: `.agents/skills/domain-modeling/SKILL.md`
- `grilling`: `.agents/skills/grilling/SKILL.md`
- `prototype`: `.agents/skills/prototype/SKILL.md`
- `research`: `.agents/skills/research/SKILL.md`
- `wayfinder`: `.agents/skills/wayfinder/SKILL.md`
<!-- skills:end -->
