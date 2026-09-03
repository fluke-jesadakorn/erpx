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

## Dev & Verify

```bash
npm install; npm run dev    # :3000 hot reload src/app/*, globals.css
npm run lint; npm run build # gate must pass (route typecheck); smoke src/app/page.tsx; optional playwright e2e
```

Env `.env/.env.local` ignored (see `.env.example`). Legacy `.runtime/target/.cargo` not used — see `backup`. Keep `.env/node_modules/.next/out` out of Git; no secrets; no Cargo without ADR.

## Structure

> Map: `docs/TOC.md` (single hub).

- `src/app/` Next.js App Router, `public/` assets
- `docs/` → `TOC.md` hub + `plan/` 31 contexts (`ARCHIVE.md` canonical) + `adr/` 21 (ADR-021 living) + `contracts/next/` + `archive/` · `audits/` stub → `docs/archive/dioxus-2026-08-evidence/`
- `Designed/mockup/` clickable prototype · `fixtures/` `ops/` (runbooks `nextjs-incident/recovery.md`) · `.agents/skills/` 6

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
