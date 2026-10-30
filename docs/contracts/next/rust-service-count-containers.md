# Native Service Count containers

The later [Parts/Count runtime qualification](service-parts-containers.md) records the shared image rebuilt after native Parts writer closure; earlier measurements below remain source-bound historical observations.

This container scope follows [the native Count delivery](rust-service-count-delivery.md), merged through [PR #5](https://github.com/fluke-jesadakorn/erpx/pull/5) into `codex/rust-backend-migration`. It contains the qualified Rust private API and shared Count projection worker. It does not claim a complete ERPX production deployment or complete contributor onboarding.

## Runtime boundary

`ops/containers/service-count-native.Dockerfile` builds the locked Cargo workspace with `--no-default-features --features private-field-api`. One binary image serves both `private-api` and `service-count-worker`. The runtime contains the native binary and exact migration inventory at the path implied by its build-time manifest directory. Node, Bun, Python, the removed TypeScript Count implementation, Cargo and fixture tools are not runtime dependencies.

Both entrypoints currently accept private leased-development configuration. The file must be mode0600 and readable by UID65532. All enabled Holdings need exact registrations and valid owned database leases; the worker additionally needs a distinct HoldingRead lease with its current migrations. This restriction is intentional. A production configuration adapter and production database provisioning are separate work, and the image does not authorize bypassing the lease checks.

The optional `ops/containers/service-count-native.compose.yaml` runs the two entrypoints from one verified image. It requires explicit verified image reference, isolated database container and private config directory variables, shares that container's network namespace, drops capabilities, uses a read-only root filesystem and a small temporary filesystem, and rotates logs. The config directory contains `private-api.json` and `worker.json`. Invoke Compose with `COMPOSE_DISABLE_ENV_FILE=1` and an explicit unique project name so it does not read an operator `.env` implicitly. It publishes no ports and manages no database or Tunnel lifecycle. The selected namespace and files must belong to the isolated case; do not point it at an operator environment.

Exact loopback URLs are preserved by sharing the isolated PostgreSQL network namespace. No broad Docker-DNS allowance, host networking, public port, or relaxed origin/lease policy is added. Next/Account rendering, mixed Inventory readers and shared Files operations remain explicitly separate transitional dependencies. There is no TypeScript Count command fallback inside this image.

## Qualification

The test-only fixture Dockerfile builds the existing native synthetic seed separately; its tools are excluded from the runtime image. `scripts/testing/fixtures/service-count-container-qualification.py` accepts explicit prebuilt local image references and the owned Docker socket. It creates uniquely labeled, disposable resources, uses private generated credentials and no host PostgreSQL port, and checks ownership before cleanup. Its Account/SSO client exercises normal authentication; synthetic fixtures do not import production accounts or data.

A container pass requires actual Account/SSO and selected Count requests, separate write/read migrations, source-bound receipts and effects, real native worker projection/ack reconciliation and graceful shutdown. Merely starting the foundation binary is insufficient. `/api/health` establishes liveness only. Global `/api/ready` remains503 with `RUST_MIGRATION_INCOMPLETE` while the full backend migration is incomplete; this scope must not turn it into a synthetic successful readiness signal.

No candidate replaces the gateway on3100 or its public Tunnel before isolated qualification. Registered public hosts, Account SSO, native Cloudflared, staging and private production remain unchanged by this container delivery. Contributor Dev Container support grows only after the corresponding native image and lifecycle are qualified; this document does not claim a fresh full-system setup, selected Next startup or HMR result.

## Measurements

Build/run measurements and supported architecture are recorded in the active delivery register after the qualification actually passes. The fixture image, builder image and final runtime size must be reported separately. A local ARM64 build does not establish AMD64 support. Images remain local; no registry publication or release is part of this delivery.

The qualified ARM64 runtime measures **16,942,946 uncompressed bytes**. The source/recipe identities and results are in [the measured evidence record](../../testing/evidence/2026-10-30-service-count-containers.json). The real isolated case passed native setup, Count creation/read and worker acknowledgement/projection under the declared hardening; it used genuine Account/SSO followed by explicitly synthetic selected-work-context fixture setup. All final-case labeled resources were removed. No unowned or pre-existing volume was pruned. A repeated cached runtime plus fixture build took 11.827 seconds; no reliable cold-build timing was recorded. Startup including seed/migrations took 10.719 seconds, and one post-command memory sample was 4.789 MiB for API and 3.426 MiB for worker. These are scoped observations, not peak load sizing or full-system performance.

Use a clean committed checkout on the owned Colima profile:

```sh
scripts/local/build-service-count-container
scripts/local/build-service-count-container --fixture
```

The fixture option additionally requires the separately prepared local verification tool image. Build output records the archived commit, architecture and recipe hashes. During review only, `--ref <commit> --recipe-worktree` explicitly selects committed Rust source with current checksummed recipes; it never copies uncommitted application source. The wrapper is necessary on legacy Docker engines that do not honor Dockerfile-specific ignore files.
