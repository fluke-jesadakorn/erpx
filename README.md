# ERPX — Rust backend migration and Next.js applications

Start with the [current production plan](docs/production-plan.md) and [ADR-052](docs/adr/ADR-052-rust-backend-and-domain-aligned-frontend.md): migrate the business backend to a Rust modular monolith and align frontend domain/application/adapter boundaries while retaining 21 independent Next.js applications. Cargo and Bun workspaces share this repository. The [documentation index](docs/TOC.md) is the single hub; historical engineering receipts certify only their recorded candidate.

The Rust implementation is **in progress**. The [migration register](docs/status/rust-backend-migration.json) records pending owners and runtime surfaces. Foundation liveness, native tests and PDF conformance cases are not a migrated business system. The TypeScript backend remains until its replacement has passed owner-level cutover checks. Docker and Dev Container onboarding follow in the second PR after the Rust migration merges; they are not available yet. See [contribution guidance](CONTRIBUTING.md), [licensing](LICENSING.md) and [public-readiness gates](docs/community/open-source.md).

The development gateway serves port3100 and starts apps lazily on owned ports3200–3299. Registered Holding hosts select independent apps by path; see [ADR-051](docs/adr/ADR-051-holding-addresses-and-workspace-paths.md). Installed production and staging are outside this migration. Full acceptance is deferred to an explicitly frozen production candidate and never runs automatically in daily CI.
Set `ERPX_DEV_MAX_APPS` in ignored `.env.local` only when the machine has room for more concurrent app processes (1–12; default4). Account and Apps share that budget. An app with active requests or HMR connections is never evicted.

## Recovery

Rust/Dioxus/Tonic/SQLx 137 crates (`:3004/:50051`) hard-cut to **Next.js 16** on `development` (`d15909bb`). Docs preserved, code not migrated. Recovery:

```bash
git checkout backup/rust-2026-09-02        # full Rust monorepo
git show rust-archive-2026-09-02 --stat    # f97b74d6
git show cc0887d --stat                    # baseline
# Historical protobuf/SQLx contracts are archived; active REST contracts and PostgreSQL migrations live on this branch.
```

Removed superseded plans, the obsolete root project review and completed documentation-triage files remain recoverable from commit `9b9349b59673beaa4f24c09f5ac11f98be70291f` with `git show <commit>:<path>`. The2026-10-27 cleanup also retained a private snapshot of the complete pre-cleanup tracked/untracked working tree and patches under ignored `.scratch/dev/repository-cleanup/`; uncommitted revisions must be recovered from that snapshot rather than inferred from a commit.

## Stack

Next.js 16.4.3 App Router + React 19.3.1 + TypeScript 7 + Tailwind 4, built as a standalone Node.js 26.11.1 release. Bun 1.4.3 manages the frontend toolchain; Rust 1.99.1 and locked Cargo dependencies provide the new backend implementation. PostgreSQL 18.6 stores relational data and durable jobs, and private filesystem adapters store evidence. The installed private production service remains on Node.js 24.20.0 until a reviewed release is deployed. Cloudflare Tunnel provides the existing installation's HTTPS ingress.

Typechecks use the TypeScript 7 native compiler. The `typescript` dependency aliases Microsoft’s TypeScript 6 compatibility package because Next.js, ESLint and repository AST tools still require its compiler API; `@typescript/native` supplies TypeScript 7 and the `tsc` command. Both are pinned in the lockfile. Security overrides keep esbuild and Zod patched and retain a patched Undici 7 for Cloudflare’s compatible API.

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

Development starts at a registered Holding: [Acme](https://acme.jesadakorn.com/), [Northstar](https://northstar.jesadakorn.com/) or [ERPX](https://erpx-internal.jesadakorn.com/). Select a Company, branch or department explicitly, then an app, for example `/bangkok/finance` or `/bangkok/finance-dept/finance`. Company work uses `/finance?company=acme-co`. Account remains central at [Account](https://account-dev-erpx.jesadakorn.com/). Former app hosts return generic404; Console retains exact health/ready reads. The [Holding development runbook](ops/macos/holding-development.md) records routing, migration and operations. Production and staging remain unchanged; full acceptance is deferred.

The rich local dataset and demo personas are documented in [docs/local-seed.md](docs/local-seed.md).

## Rust migration verification

Install the toolchain pinned by `rust-toolchain.toml`, then use the native commands below. The inventory command reads implementation metadata and does not start a service or change a database.

```bash
bun run rust:test
bun run rust:fmt
bun run rust:check
bun run rust:clippy
cargo run --locked --package erpx-service --bin erpx-server -- owners
```

The native unit command uses catalogue targets and case receipts without database access. PostgreSQL suites require a configured local test administrator and explicit integration selection; the fixture creates and drops only its leased disposable databases. Use the registered test catalogue for affected native/TypeScript checks. Ordinary development does not run production acceptance, deploy services or publish images. Follow [SECURITY.md](SECURITY.md) for vulnerability reports and [GOVERNANCE.md](GOVERNANCE.md) for ownership and architecture decisions.

## Structure

- `apps/<id>/`: app-owned routes, layouts, handlers, config, route types and build output; no imports from another app. `apps/compat` retains non-product compatibility source. The authorized development Console cutover is active; its pages/APIs return404 and health/ready reads use Account. Holding paths dispatch to their owning applications. Apps keeps the stable `apps` app/CLI identity in `apps/launcher/`.
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

Use `bun run docs:check`, `bun run products:check`, `bun run structure:check`, `bun run test:catalogue`, and `bun run architecture:check` for repository checks. Documentation checks include root Markdown and the archive index while preserving immutable evidence; tracked temporary files are rejected. The per-app development profile runs lint/typecheck, related unit/integration and registered browser smoke. [E2E specification](docs/testing/e2e-specification.md) records case identities and remaining gaps.

`bun run release:acceptance --candidate=<sha>` is reserved for intentional production preparation. It refuses an unfrozen candidate or a missing combined app/gateway/jobs release; the workspace-v2 acceptance adapter remains an explicit delivery blocker. It must not run during this migration. Existing dated receipts and specialized UI gates remain unchanged, and development observations never promote them.

Browser traces, logs, process leases and benchmark receipts remain private under ignored `.scratch/dev/`. Use the existing authorized dev account through normal sign-in; prepare fictional fixture accounts within isolated browser cases.
