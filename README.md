# ERPX — Next.js (rebuilt 2026-09-02)

Start with the [current production plan](docs/production-plan.md): engineering completion of the current 33 modules and 187 capabilities, with a fixed 12-task closure checklist. The completed engineering run dated 2026-10-03 verified all 187 capabilities on its recorded source/candidate; later revisions are not automatically reverified. [Documentation index](docs/TOC.md).

Active deployment: [Node.js/PostgreSQL on this Mac through Cloudflare Tunnel](ops/macos/README.md), per [ADR-024](docs/adr/ADR-024-local-postgresql-runtime.md). Cloudflare Workers configuration is historical. Use `bun run deploy:build` for an isolated release build; building does not authorize deployment.

Rust/Dioxus/Tonic/SQLx 137 crates (`:3004/:50051`) hard-cut to **Next.js 16** on `development` (`d15909bb`). Docs preserved, code not migrated. Recovery:

```bash
git checkout backup/rust-2026-09-02        # full Rust monorepo
git show rust-archive-2026-09-02 --stat    # f97b74d6
git show cc0887d --stat                    # baseline
# Historical protobuf/SQLx contracts are archived; active REST contracts and PostgreSQL migrations live on this branch.
```

## Stack

Next.js 16 App Router + React 19 + TypeScript + Tailwind 4, built as a standalone Node.js 24.20.0 release. Bun 1.2.17 manages the toolchain; PostgreSQL 18.6 stores relational data and durable jobs, and private filesystem adapters store evidence. Cloudflare Tunnel provides HTTPS ingress. No Cargo.

```bash
bun install --frozen-lockfile
# On a fresh checkout/source archive, generate Next's ignored type declarations.
# Use the pinned Node 24.20.0 executable on PATH.
node node_modules/next/dist/bin/next typegen
# Configure absolute data/manifest paths and a separate development PostgreSQL URL first.
bun run seed:local           # development data only; never production
bun run seed:verify:local   # read-only seed checks
bun run dev                # :3100 UI development
bun run deploy:build       # isolated production release
bun run lint; bun run typecheck; bun run test:integration
```

The rich local dataset and demo personas are documented in [docs/local-seed.md](docs/local-seed.md).

## Structure

`src/app/` (workplace and APIs) · `src/server/` (policy/domain) · `scripts/local/` (jobs and operations) · `migrations/postgres/` · `ops/macos/` · `docs/` hub (`docs/TOC.md`) · `Designed/mockup/` reference · `fixtures/` · `ops/`

## Business context

[Production plan](docs/production-plan.md) → [current contracts](docs/contracts/next/README.md), [glossary](CONTEXT.md), and [architecture decisions](docs/adr/README.md). The generated catalogue defines workflow ownership; the verification inventory separates dated engineering evidence, provider scope and Company release acceptance.

## Verification

`bun run lint`, `bun run typecheck`, `bun run deploy:build`, and `bun run test:integration` must pass. Shared development additionally requires deployed smoke and Playwright journeys.

### Playwright end-to-end tests

Use Node 24.20.0 and Bun 1.2.17. Install Chromium once with
`bunx playwright install chromium`. Reuse the development server on port 3100,
or start `bun run dev` after checking that the port is free. Playwright uses
`http://127.0.0.1:3100` by default and does not manage the server lifecycle.

```bash
bun run test:e2e:smoke                 # authentication smoke, desktop + mobile
bun run test:e2e:work                  # Work filters, recovery and request draft lifecycle
bun run test:e2e:smoke --project=desktop
bun run test:e2e:ui                    # interactive test runner
bun run test:e2e:report                # last HTML report
bun run test:e2e:local                 # all existing business suites
```

The smoke suite checks health, signed-out route protection, rejected credentials,
account recovery/signup navigation, and UI sign-in with session persistence.
Local sign-in uses the existing seeded Mali account; prepare development data
with `bun run seed:local` if needed. Set `ERPX_E2E_EMAIL` and
`ERPX_E2E_PASSWORD` to use another account with holding home access.
It does not create business records or send recovery emails.

The [E2E specification and test cases](docs/testing/e2e-specification.md) maps
current source behavior to executable cases and records remaining coverage gaps.
The Work suite uses local seeded Mali, creates uniquely named service drafts,
and cancels them while preserving audit history. It skips remote targets.

Set `ERPX_E2E_BASE_URL` to test another running deployment. Remote authenticated
smoke tests require explicit account environment variables; otherwise that test
is skipped. Existing business suites have separate fixture requirements (see
each spec and `scripts/seed-*-e2e.ts`), and some still depend on historical
Cloudflare fixtures; passing smoke tests does not certify those suites.
Failure screenshots and traces are saved under ignored `test-results/`, with an
HTML report under `playwright-report/`. Traces can contain session data; keep
them private.
