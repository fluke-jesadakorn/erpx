<!-- BEGIN:nextjs-agent-rules -->

# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` (resolved from this file's directory; in monorepos the `next` package may not be visible from the repo root) before writing any code. Heed deprecation notices.

This block is written and re-added by `next dev` — verify at `node_modules/next/dist/server/lib/generate-agent-files.js`. Removing it from a diff only re-creates the uncommitted change; committing it with your work keeps the tree clean.

<!-- END:nextjs-agent-rules -->

# AGENTS.md

ERPX is a Next.js workspace rooted at this directory (rebuilt from Rust/Dioxus on 2026-09-02).

## Tooling

Use Bun 1.2.17 with the pinned Next.js/Node.js toolchain. The deployed runtime is Node.js 24.20.0 standalone with PostgreSQL 18.6, private local files and durable PostgreSQL jobs behind Cloudflare Tunnel (ADR-024). Rust/Cargo/Dioxus/Tonic/SQLx are no longer part of this branch — full Rust source is archived on `backup/rust-2026-09-02` tag `rust-archive-2026-09-02` and baseline `cc0887d` per `CUTOVER.md`.

```bash
bun --version  # 1.2.17
bun run dev    # Next.js on http://localhost:3100
bun run typecheck
bun run lint
bun run deploy:build
```

## Dev & Verify

```bash
bun install --frozen-lockfile; bun run dev
bun run lint; bun run build; bun run deploy:build
bun run db:migrate:local; bun run db:seed:local; bun run test:integration
```

## Form presentation

Long forms opened from the current page must use a right drawer, not a centered modal or an expanded inline form. Use `Dialog presentation="form-drawer"`, `FormDrawer` for editors shared with standalone pages, or `FormDisclosure` for disclosure triggers. Keep dedicated full-page forms inline. Small confirmations, search, and pickers may remain dialogs. Drawers keep a fixed header, scroll their body, and use the full viewport width on small screens.

## Port safety

Ports `3000`, `3001`, and `8080` are reserved for other projects. Never start ERPX or any ERPX helper service on those ports, and never stop the processes using them.

- Next.js development: `3100` → `https://console-dev-erpx.jesadakorn.com`
- Staging standalone: `3103` → `https://erpx.jesadakorn.com`
- Private production standalone: `3101` (loopback only)
- Shared PostgreSQL 18.6: `5432` (dev `erpx_dev_*`, private production `erpx_*`, staging `erpx_stage_*` databases)
- Retired ERPX PostgreSQL ports: `5433` and `5435` (disabled; source data directories retained for rollback)

Check that the intended port is available before starting a service. Reuse an existing ERPX process when it is already running. If `3100` is occupied by an unrelated process, report the blocker; do not terminate it, take production port `3101`, or silently change the public origin.

## Required end-of-work environment availability

Use `https://console-dev-erpx.jesadakorn.com/` for development console work and browser verification; it serves this workspace's development server at `http://127.0.0.1:3100`. Keep the installed production web/jobs service on `3101` private and the shared PostgreSQL service on `5432` available. Do not restart the retired `5433`/`5435` clusters during normal work.

Do not touch staging unless the user explicitly instructs you to do so. This includes staging browser visits, health checks, deployments, builds in staging directories, migrations, configuration or route changes, and starting, stopping, or restarting staging services. Staging uses port `3103`, `https://erpx.jesadakorn.com`, `https://console-erpx.jesadakorn.com`, and `erpx_stage_*` databases. Routine development work and end-of-work verification do not authorize staging access.

- Before starting dev, inspect its port and process working directory. Reuse a healthy ERPX process; never start duplicate servers. Apply the same checks to staging only when the user explicitly requests staging work.
- After completing work and verification, leave `bun run dev` running on `3100` in a detached background process that survives the command/task ending. Use pinned Node 24.20.0 and Bun 1.2.17. Keep logs and PID information under ignored `.scratch/dev/`; never print secrets from logs.
- If a build, configuration change, or verification requires stopping dev, restart it before finishing. Do not run a build against the same Next output directory while dev is using it; use the isolated release build or stop and restore dev.
- Start and verify the authorized environment locally before changing its Cloudflare routes. Preserve other Tunnel routes and services; staging route changes require an explicit user instruction.
- Before the final response, verify HTTP 200 from `http://127.0.0.1:3100/api/health`, `https://console-dev-erpx.jesadakorn.com/api/health`, and the private production service at `http://127.0.0.1:3101/api/health`. Also check the affected page through `https://console-dev-erpx.jesadakorn.com/`. Check staging health and pages only when the user explicitly requests staging work.
- For UI work, inspect the actual browser page and confirm the requested appearance/behavior. Health checks alone do not establish completion. Use the user's existing authenticated session when available; never bypass authentication. Report explicitly if login prevents verification.
- Recheck the listener and HTTP response after the startup command has returned, so an initial startup message is not mistaken for a persistent server.
- On 502, 404, or other failures, inspect relevant logs and fix the issue before claiming success. If blocked, state exactly what remains unverified or broken. Include the public URL and actual verification results in the final response.

This is an end-of-work availability requirement, not an uptime guarantee or an instruction to install an automatic boot service.

Env `.env/.env.local` ignored (see `.env.example`). Legacy `.runtime/target/.cargo` not used — see `backup`. Keep `.env/node_modules/.next/out` out of Git; no secrets; no Cargo without ADR.

## Structure

> Map: `docs/TOC.md` (single hub).

- `src/app/` Next.js App Router, `src/server/` domain/policy services, `src/workers/` queue/scheduled handlers, `public/` assets
- `migrations/postgres/` active schema tracks; `ops/macos/` native deployment; remaining Wrangler/D1 fixtures serve the historical E2E adapter. Removed Workers/OpenNext tooling is recoverable through `CUTOVER.md`.
- `docs/production-plan.md` is the sole active delivery plan; `TOC.md` is the entry point. `contracts/next/` owns current behavior, `CONTEXT.md` vocabulary, `adr/` decisions, and `archive/` immutable dated evidence. Superseded plans are recoverable through the verified Git references in `CUTOVER.md`. The generated capability inventory is a reference, not additional pilot scope.
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
