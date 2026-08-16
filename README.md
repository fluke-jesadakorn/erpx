# ERPX

ERPX is a Web-first Rust monorepo with a Dioxus console, Tonic APIs, durable
workers, SQLx persistence, native identity, and shared crates. The Web console
is the certified product target in this phase and is served at
<http://localhost:3004>. Existing Desktop/Mobile feature definitions remain for
compatibility, but their builds and certification are deferred.

## Local development

PostgreSQL, MinIO, Ollama, and n8n are host-native services. Keep `.env`,
`.env.local`, and `.runtime/` outside Git. In particular, never delete or
replace `.runtime/n8n/`: it contains the n8n state and encryption key.

The local database credentials are read from `.env.local`:

```dotenv
POSTGRES_HOST=127.0.0.1
POSTGRES_PORT=5432
POSTGRES_USER=contract
POSTGRES_PASSWORD=change_me
POSTGRES_DB=folio_db
ERPX_DEV_LEGACY_MINIO_ENDPOINT=http://127.0.0.1:9000
ERPX_DEV_LEGACY_MINIO_ROOT=/absolute/path/to/local/minio-data
```

The destructive development reset is bounded to the loopback database and
object-store targets declared above. It prints its exact targets, rejects
unknown schemas or buckets, and requires the literal confirmation:

```bash
cargo xtask reset-dev --confirm RESET_ERPX_DEV
cargo xtask seed-demo
cargo xtask dev --detach
```

`seed-demo` is idempotent. On the first run it prints the local Platform Root
bootstrap password and recovery code once; only their hashes are stored.
Directory connector support (LDAP/RFC4533, AD DS/DirSync, and UCS) is compiled
but remains disabled in the development profile. External providers are
unconfigured and never report synthetic success.

## Runtime commands

```bash
cargo xtask dev --detach  # hot-patch Web, debug-off; reuse healthy services
cargo xtask dev --detach --fresh  # full non-destructive stop/reconcile/start
cargo xtask dev --detach --no-hot-patch  # stable Dioxus fallback
cargo xtask dev --detach --web-debug  # enable Web debug symbols/line tables
cargo xtask status        # report only recorded ERPX child processes
cargo xtask stop          # stop only recorded ERPX child processes
cargo xtask seed-demo     # idempotent development fixture
cargo xtask dev-access credentials  # explicitly reveal the local controller login
cargo xtask pitr rehearsal # temporarily enable bounded PITR, verify restore, then purge artifacts
cargo xtask pitr cleanup-local --confirm PURGE_ERPX_LOCAL_PITR # purge the fixed local PITR chain
cargo xtask check-affected # quick owning-package compile checkpoint; no tests or linking
cargo xtask test-affected # test only changed packages, consumers, and domain harnesses
cargo xtask verify-core   # offline CI-parity core gate (after cargo fetch)
cargo xtask verify-runtime # strict process, heartbeat, and projection evidence
cargo xtask verify-sqlx   # disposable PostgreSQL acceptance lane
cargo xtask verify        # serialized full Web gate
cargo xtask verify-web-production # additive release-only production gate
cargo xtask verify-platforms # manual deferred Desktop/Mobile checks
```

Runtime logs and PID records are under ignored `.runtime/erpx/`. By default the
Tonic API listens on `127.0.0.1:50051`, HTTP auth/health on
`127.0.0.1:50052`, and the Dioxus console on `127.0.0.1:3004`.
When `ERPX_DEV_PERSONA_MODE=enabled`, the debug console selects
`local:dev-controller` on the sign-in screen. Its generated password remains in
the private runtime secret file and is printed only by the explicit
`cargo xtask dev-access credentials` command.

Continuous PITR archiving is disabled by default on local development. A
`pitr rehearsal` acquires a private two-hour lease with a 4 GiB WAL budget,
verifies LSN/time recovery, and removes its base backup and WAL artifacts on
exit. `cargo xtask status` reports the lease and retained spool/MinIO usage;
the literal-confirmation cleanup command is the only manual purge entry point.

For dev-only access over Tailscale, keep ERPX loopback-only and let Tailscale
Serve forward the three development ports. Enable the development persona mode
and set the machine's exact Tailscale IPv4 address; wildcard, LAN, public, and
addresses outside Tailscale's `100.64.0.0/10` range are rejected:

```dotenv
ERPX_DEV_PERSONA_MODE=enabled
ERPX_DEV_TAILSCALE_IP=100.97.9.56
```

Configure tailnet-only TCP forwarding once, then start ERPX:

```bash
tailscale serve --bg --tcp=3004 tcp://127.0.0.1:3004
tailscale serve --bg --tcp=50051 tcp://127.0.0.1:50051
tailscale serve --bg --tcp=50052 tcp://127.0.0.1:50052
cargo xtask dev --detach
```

ERPX itself remains bound to loopback; Tailscale owns the tailnet listeners and
the console uses the configured address for browser auth and gRPC-Web traffic.

### Fresh-state CQRS cutover

`erpx-bootstrap` is the one-shot installer for the generation-v2 target runtime. It
creates a new control write/read pair, a UUID-tenant write/read pair, and a new
MinIO namespace. It seeds only Platform Root, Holding Owner, the tenant/holding
registry, canonical business units, and the neutral published brand. It never
copies identity, RBAC, business, or object data from retained resources.

The complete environment contract is documented in `.env.example`. Run
`erpx-bootstrap check` first, stop the recorded ERPX runtime, then run
`erpx-bootstrap bootstrap`. A successful bootstrap writes the non-secret,
write-once `.runtime/erpx/cutover/fresh-state.json` manifest. `cargo xtask dev
--detach` accepts that manifest only when the matching database bootstrap
receipt is complete, the tenant registry row is active, the schema and service
catalog generations are both `2`, the service range is `IFC-SVC-101..165`,
rollback is offline/no-merge, and the exact n8n config hash is unchanged. Fresh
mode skips legacy database reconciliation and business-data demo
fixtures. When `ERPX_DEV_PERSONA_MODE=enabled` is explicitly configured, the
development runtime generates the gated controller, secret-free QA persona catalog,
and local-only role assignments directly in the active fresh control database after
the receipt is validated. It never copies identities, RBAC, or business data from
retained resources, and the bootstrap seed itself remains unchanged. The
`dev-access` reconcile, status, rotate, and reset commands resolve that same active
control database.

Rollback uses the retained binaries, environment snapshot, databases, and
object namespaces as an isolated system. Old and new writes are never merged.
Neither bootstrap nor the runtime handoff deletes retained resources or
modifies `.runtime/n8n/`.

The default local data plane is host-native and shared: one PostgreSQL 18
cluster on `55432`, one HTTPS MinIO instance on `9000/9001`, and one UCS lab
process on `15443`. Every local layer reuses one PostgreSQL application
credential, one MinIO credential, and one UCS password at those same endpoints.
Separation is logical: Platform uses the control database pair and Platform
object namespaces; each Holding uses its own database pair, MinIO bucket, and
UCS authority. Business Units inherit the Holding resources through RLS scope,
object-key prefix, and directory group-to-role mappings. No Docker or Compose
process is started.

The generated, secret-free topology is
`.runtime/erpx/infra/local-topology.json`; shared private material remains under
`.runtime/erpx/secrets/`. `cargo xtask status` reports the physical and logical
topology without printing credentials. A first transition uses
`cargo xtask dev --detach --fresh`; later starts reuse all three infrastructure
instances.

Start the stack once and leave Dioxus hot reload running. Web development uses
one monolithic WASM with Rust hot-patch and debug symbols disabled by default,
and keeps its persistent session cache in `target/dx-hotpatch`. Source, RSX,
CSS, and asset edits are left to the running Dioxus watcher; `xtask` restarts
Web only when its environment, workspace manifests/toolchain, DX version, or
launch mode changes. Use `--no-hot-patch` for a stable monolithic rebuild and
`--web-debug` when runtime line tables are needed. Optional flags may appear in
any order and duplicates or unknown flags fail closed.

After backend edits, run `cargo xtask dev --detach` again: healthy
infrastructure and Web are reused, current fixtures are skipped, and only
changed or missing backend service groups restart. Use `--fresh` when local
runtime configuration must be fully reconciled; it does not drop databases or
buckets and never touches `.runtime/n8n/`. `cargo xtask status` includes the
active Web launch mode (`hot-patch`/`stable`, `debug-on`/`debug-off`).
The status line always identifies the Web layout as `monolithic`.

For the daily feedback loop, keep `cargo xtask dev --detach` running and let
Dioxus hot reload handle each Rust/RSX/CSS/asset edit. Do not run Cargo tests
after every UI edit. At a small checkpoint run the quick compile lane:

```bash
cargo xtask check-affected --dry-run
cargo xtask check-affected
```

`check-affected` stores an independent successful snapshot at
`.runtime/erpx/affected-checks.json`, seeded initially from the last successful
test/verify snapshot. It format-checks changed Rust files and uses `cargo check`
for only the owning library, changed binary, or exact integration target. It
does not link or run tests, scan architecture, or walk reverse dependencies.
Console checks exactly one feature combination (`web`, or `server` for
server-only source). CSS/HTML/image-only changes stay with Dioxus hot reload.
Root manifests, the lockfile/toolchain, protobuf, migrations, and shared
contract changes explicitly escalate outside the quick path to a workspace
check. Selector, rustfmt, Cargo, and total timings are printed on every run.
A failed quick check never advances either snapshot, and a successful quick
check never causes the thorough lane to skip tests.

Finish a related change set with the thorough affected lane:

```bash
cargo xtask test-affected --dry-run
cargo xtask test-affected
```

The selector stores its successful snapshot in ignored `.runtime/erpx/`, uses
locked/offline Cargo metadata, formats only changed Rust files, tests changed
package library targets and only explicitly changed bin targets, compile-checks
reverse dependents, and dispatches only the owning domain harness. A changed
integration-test file runs its own exact test binary. A Console feature change
checks the thin Console consumer once; broader Console Rust changes retain the
`web` and `server` checks;
Console CSS/HTML/image-only changes do not start Rust workspace tests. Shared
contracts, protobuf, migrations, root manifests, CI, or toolchain changes
escalate to the workspace fast lanes and architecture scan. The snapshot moves
only after all selected checks pass.

Use the broader fast lanes explicitly when diagnosing or validating a shared
boundary:

```bash
cargo check-fast
cargo test-fast
cargo xtask architecture
cargo xtask test <harness> [filter]
```

The fast aliases cover workspace library and binary targets with default Web
features and intentionally exclude the deferred Android bridge, integration-test
executables, and doctests. ERPX workflow commands use one advisory Cargo lock and
fail with the owning PID/command instead of starting a competing build. Direct
Cargo commands remain available for focused, uncoordinated diagnosis.

ERPX keeps allowlisted Cargo build caches under a 128 GiB budget while preserving
at least 64 GiB of filesystem free space. The lightweight
`xtask`, `check-fast`, and `test-fast` launchers share
`.runtime/erpx/xtask-target`; heavy orchestration builds in
`.runtime/erpx/heavy-xtask-target`, native development services build and run
from `.runtime/erpx/runtime-dev-target`, and normal workspace checks use root
`target/`. Managed build
commands update each cache group's last-used marker and automatically remove
the least recently used complete target groups when the effective budget is
exceeded. Active process pins are separate from warm validation pins, so a
Dioxus/runtime pin does not evict the root validation cache merely because the
workflows alternate.
An evicted target is safe to rebuild, but its next compilation will take
longer. A recorded, active Dioxus hot-reload process pins the WASM and Dioxus
target groups it may write, and recorded native ERPX processes pin their debug
artifacts. Pruning refuses to race an active workspace `rustc` writer.

```bash
cargo xtask cache status
cargo xtask cache prune --dry-run
cargo xtask cache prune
cargo xtask cache prune --incremental --dry-run
cargo xtask cache prune --incremental
cargo xtask cache prune-debug --dry-run
cargo xtask cache prune-debug
```

The cache allowlist contains only immediate groups below `target/` and the
dedicated Cargo target directories below `.runtime/erpx/`; runtime data,
databases, MinIO state, logs, and `.runtime/n8n/` are never candidates. Direct
Cargo commands do not run the janitor themselves, but their target groups are
included the next time an ERPX-managed build command runs. Manual pruning also
removes superseded fast-lane target groups and regenerable deferred iOS/Android
target groups. Incremental pruning is separately opt-in and refuses to run while
Dioxus or a workspace compiler is active. `prune-debug` is the narrower
migration cleanup: it selects only root `target/debug` and refuses to run while
a recorded native ERPX process still uses that legacy path. Active Web,
isolated native runtime, and heavy-xtask target groups are pinned independently.

The 19 integration harnesses are grouped by domain:

- Infrastructure: `infra_platform`, `infra_tenant_runtime`, `infra_finance`,
  `infra_marketing`, `infra_operations`, `infra_people_support`, `infra_backup`,
  `infra_migration`.
- Worker: `worker_platform`, `worker_finance_operations`, `worker_directory`.
- Migrator: `migrator_products`, `migrator_cutover`.
- Identity: `identity_core`, `identity_acceptance`.
- Domain: `domain_contracts`.
- UI: `ui_domains`, `ui_kit`, `ui_platform`.

Development builds keep Cargo incremental compilation with debug information
disabled by default. `--web-debug` enables line tables only for the Dioxus Web
profiles. The test profile remains unchanged and disables incremental state,
preventing full test lanes from accumulating another large incremental cache.
Release WASM uses the dedicated `wasm-release` Cargo profile with optimization
level 3, fat LTO, one codegen unit, and no debug information, followed by
`wasm-opt -O4`. `cargo xtask wasm-size --dev|--release` Brotli-compresses and
reports the main module size as informational evidence while failing any layout
that is not exactly one WASM or that retains split glue, chunks, or lazy module
files.

## Workspace

- `apps/server`: Tonic services and HTTP auth/health endpoints
- `apps/worker`: durable projectors, schedulers, and reconciliation workers
- `apps/console`: Dioxus Web console; Desktop/Mobile features are deferred
- `apps/migrator`: generic legacy reconciliation and rollback tooling
- `crates/kernel`, `crates/cqrs`, `crates/integration-contracts`: shared
  target-neutral contracts and CQRS foundations
- `crates/domain`, `crates/application`: product contracts and use cases
- `crates/infrastructure`: SQLx repositories and provider adapters
- `crates/identity`: native identity, MFA, and directory connectors
- `crates/migration`: control/tenant write/read migrations
- `crates/proto`: public gRPC contracts and descriptor baseline
- `crates/ui` (`erpx-ui-core`), `crates/ui-kit`, and `crates/ui-domains/*`:
  independently compiled UI core, component kit, and domain presentation models
- `crates/ui-facade` (`erpx-ui`): compatibility re-export surface for existing clients
- `crates/console-shell`, `crates/console-transport`, `crates/console-bff`, and
  `crates/console-features/*`: Console shell/wire/BFF boundaries and 21 independent features
- `audits`: retained ERPX implementation and acceptance evidence
- `audits/CURRENT-CERTIFICATION.md`: current certification scope and latest gate result
- `xtask`: bounded development lifecycle and verification

## Verification

```bash
cargo xtask verify
```

`cargo xtask verify` is the single pre-handoff entry point. It serializes
`cargo xtask verify-core` (formatting, warnings-denied workspace Clippy, the
full workspace test suite, architecture/repository scans, explicit Console
`web` and `server` checks, a locked offline Dioxus Web release build, and its
single-WASM layout check), then
local SQLx acceptance, strict process/heartbeat/projection readiness, and
runtime HTTP/SSR/BFF smoke checks. A successful full gate refreshes the affected
baseline even for a dirty worktree. CI fetches locked dependencies first, runs
the same offline core gate, and follows it with the disposable PostgreSQL lane.
See [the current certification index](audits/CURRENT-CERTIFICATION.md); dated
audit directories and `CUTOVER.md` are retained historical evidence.

The release-only production gate is additive:

```bash
cargo xtask verify-web-production
```

It first runs the complete normal Web gate, then fails closed unless the Web
production profile uses HTTPS-only trusted/auth/gRPC endpoints, JSON structured
logs, non-local session-key references, valid secret material, and no
development-persona configuration. It verifies the deployed login/protected/
unknown routes, security headers, health and metrics endpoints, worker/outbox/
projection thresholds, and live backup/PITR metric evidence. The versioned JSON
file named by `ERPX_WEB_PRODUCTION_EVIDENCE` must match
`ERPX_WEB_PRODUCTION_REVISION` and include one schema-v2 journey for each of the
12 modules. Every journey must evidence login, scope, list/detail, create,
lifecycle transition, read-your-writes, audit, session recovery, authorization,
validation, conflict, duplicate submission, recovery, English/Thai, desktop/
390px, and zero unexplained console errors. The same gate requires zero
critical/high security findings, backup restore, object recovery, PostgreSQL
PITR, loaded alerts, and verified runbooks.
Use [the evidence schema example](ops/web-production-evidence.example.json) and
the runbooks under `ops/runbooks/`; the example is a template, never release
evidence.

Run `cargo xtask verify-platforms` only when manually assessing the deferred
Desktop, generic Mobile, iOS, Android, and Android bridge targets. It is not
called by the daily loop, normal CI, or the final Web gate. Those platforms are
not certified in this phase.

Non-loopback database, object-store, provider, and directory endpoints must use
certificate-validating TLS. External directory certification remains required
only when that integration is enabled for the launch tenant. Desktop/Mobile
certification remains outside the Web production target.
