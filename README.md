# ERPX — Next.js (rebuilt 2026-09-02)

Start with the [current production plan](docs/production-plan.md): the in-progress migration to22 independent Next.js apps in Bun workspaces and a scoped development/test platform. [ADR-049](docs/adr/ADR-049-product-app-workspaces-and-development-verification.md) records the architecture; [documentation index](docs/TOC.md) is the single hub. Historical engineering receipts certify only their recorded candidate.

The development gateway serves port3100 and starts apps lazily on owned ports3200–3299. Each host switches only after its scoped checks and browser smoke pass. Installed production and staging are outside this migration. Full acceptance is deferred to an explicitly frozen production candidate and never runs automatically in daily CI.
Set `ERPX_DEV_MAX_APPS` in ignored `.env.local` only when the machine has room for more concurrent app processes (1–12; default4). Account and Apps share that budget. An app with active requests or HMR connections is never evicted.

## Recovery

Rust/Dioxus/Tonic/SQLx 137 crates (`:3004/:50051`) hard-cut to **Next.js 16** on `development` (`d15909bb`). Docs preserved, code not migrated. Recovery:

```bash
git checkout backup/rust-2026-09-02        # full Rust monorepo
git show rust-archive-2026-09-02 --stat    # f97b74d6
git show cc0887d --stat                    # baseline
# Historical protobuf/SQLx contracts are archived; active REST contracts and PostgreSQL migrations live on this branch.
```

## Stack

Next.js 16.4 App Router + React 19.3 + TypeScript + Tailwind 4, built as a standalone Node.js 26.11.1 release. Bun 1.4.3 manages the toolchain; PostgreSQL 18.6 stores relational data and durable jobs, and private filesystem adapters store evidence. The installed private production service remains on Node.js 24.20.0 until a reviewed release is deployed. Cloudflare Tunnel provides HTTPS ingress. No Cargo.

```bash
bun install --frozen-lockfile
# Use pinned Node 26.11.1 and Bun 1.4.3 on PATH.
# Each app generates its own ignored route types during dev/build.
# Scoped app typechecks generate routes in a private project alongside dev.
# Configure absolute data/manifest paths and a separate development PostgreSQL URL first.
bun run seed:local           # development data only; never production
bun run seed:verify:local   # read-only seed checks
bun run dev                              # lazy apps behind :3100
bun run dev -- --app=finance              # prewarm one product
bun run test:dev                          # staged/unstaged/untracked changes vs HEAD
bun run test:dev --app=finance            # scoped development profile
bun run test:dev --watch                  # cancel obsolete rounds after edits
bun run test:ci --base=<sha> --head=<sha>  # clean exact candidate verification
bun run build:affected                   # independent app build outputs/cache
```

Development starts at [ERPX Apps](https://apps-dev-erpx.jesadakorn.com/). All22 hosts use their own app processes; the retired development Console and local Console business surface return a generic404 without Console branding or a migration page. Console health remains200 through Account. Production and staging remain unchanged. Installed post-cutover visual verification and the remaining platform/performance gates are tracked in the production plan; full acceptance is deferred.

The rich local dataset and demo personas are documented in [docs/local-seed.md](docs/local-seed.md).

See [Contributor container setup](ops/containers/README.md) for the transitional isolated contributor environment and its qualification status.

## Structure

- `apps/<id>/`: app-owned routes, layouts, handlers, config, route types and build output; no imports from another app. `apps/compat` retains non-product compatibility source. The authorized development Console cutover is active; its pages/APIs return404 and health/ready reads use Account. Product hosts use their own applications. Apps keeps the stable `apps` host/CLI identity in `apps/launcher/`.
- `packages/server-<owner>/src/{domain,application,adapters,public}/`: in-process business owners; consumers use named public exports and typed ports.
- `packages/feature-<owner>/`, `packages/ui/`, `packages/workplace/`, `packages/front/`: owner presentation, primitives and shells with explicit subpath exports. `packages/contracts/` owns pure host/product metadata and shared contracts.
- `tests/{unit,integration,e2e,journeys,uat,provider}/<owner>/`: executable suites, assigned from the catalogue rather than filenames. `tests/support/` holds helpers/setup; `tests/catalogue/` holds required-case registers; `tests/config/` holds runner/typecheck configuration.
- `scripts/{testing,performance,seed,docs,assets,ui,local}/`: executable tooling by purpose. `fixtures/<owner>/`: fixture data and builders shared by seeds and tests. Executable fixture preparation belongs in `scripts/testing/fixtures/`.
- `migrations/postgres/`: existing database tracks; `ops/`: operations; `docs/TOC.md`: documentation hub, with research in `docs/research/` and verification in `docs/testing/`.

After changing workspace UI dependencies, run `bun run workspaces:styles` to refresh Tailwind source ownership; scoped verification rejects stale style inputs. App source imports must use declared package subpath exports.

Register every test in `tests/catalogue/suite-manifest.json`, place it under its declared lane and owner, and keep test support outside executable suite folders. Use the owner's public entry point rather than adding legacy forwarding modules. `bun run structure:check` verifies these placement rules. The sole retained legacy runtime entry supports the unchanged staging bootstrap.

## Business context

[Production plan](docs/production-plan.md) → [current contracts](docs/contracts/next/README.md), [glossary](CONTEXT.md), and [architecture decisions](docs/adr/README.md). The generated catalogue defines workflow ownership; the verification inventory separates dated engineering evidence, provider scope and Company release acceptance.

## Verification

Daily verification uses affected suites and registered desktop browser smoke with `acceptance:false`. Unit/typecheck/lint/build tasks may use complete-input caches; selected integration and browser cases execute against isolated fictional fixtures. Missing dependency/suite mappings fail selection. Migration tests retain real migrations against empty databases.

Use `bun run structure:check`, `bun run test:catalogue`, and `bun run architecture:check` for structural checks. The per-app development profile runs lint/typecheck, related unit/integration and registered browser smoke. [E2E specification](docs/testing/e2e-specification.md) records case identities and remaining gaps.

`bun run release:acceptance --candidate=<sha>` is reserved for intentional production preparation. It refuses an unfrozen candidate or a missing combined app/gateway/jobs release; the workspace-v2 acceptance adapter remains an explicit delivery blocker. It must not run during this migration. Existing dated receipts and specialized UI gates remain unchanged, and development observations never promote them.

Browser traces, logs, process leases and benchmark receipts remain private under ignored `.scratch/dev/`. Use the existing authorized dev account through normal sign-in; prepare fictional fixture accounts within isolated browser cases.
