# Service Count contributor Dev Container

This profile extends the qualified [native Count](rust-service-count-delivery.md) and [minimal Docker runtime](rust-service-count-containers.md). It supports Account/Apps, explicit Acme Company/role selection, Service parts prerequisites, initial physical stock Count draft creation/readback and native Count projection. It is a development environment with compilers and hot reload. It does not establish migration of all Field workflows, all owners, AMD64 qualification or production readiness.

## Supported host and isolation

The tested path is macOS ARM64 with the dedicated Colima profile `erpx-opensource-dev`, the standalone `docker-compose` command, Git and the Dev Containers CLI/editor integration. The lifecycle deliberately requires that profile's socket at `$HOME/.colima/erpx-opensource-dev/docker.sock`. Generic Linux/Windows engines and bind-mounted source are not qualified by this profile. Native/hybrid development remains available through the repository's ordinary tooling; it is a separate path, not an equivalent fresh-container claim.

Use a unique lowercase key for each checkout, architecture and toolchain. The profile checks available host disk headroom and loopback3443 before creating services; it never stops another listener or silently changes ports. A running dedicated VM with6 CPU,16GiB RAM and80GiB disk was used for qualification. Resource sizing is an observation, not a minimum guarantee.

```sh
export ERPX_DEV_WORKSPACE_KEY=my-count-arm64-rust1991
# Use the reviewed branch/commit you cloned; the source volume is independently cloned.
export ERPX_DEV_SOURCE_REF=$(git rev-parse HEAD)
scripts/local/dev-container up --app=service
```

The first run builds the pinned toolchain image when absent, clones credential-free HTTPS source into a Linux volume, installs locked dependencies, migrates eight isolated databases, creates fictional seed data and starts Rust plus Account/Apps/Service lazily. It does not copy the host working tree, ignored environment files, operator credentials or production data. Uncommitted host edits are intentionally not imported. Edit `/workspace/erpx` in the container workspace thereafter.

The workspace includes Node26.11.1, Bun1.4.3 and Rust1.99.1. PostgreSQL18.6, scanner, local mailbox and Caddy are separate services. The API and Count worker are separate processes in the shared toolchain workspace, running as UID65532 with private configuration. This is not a container per owner; the small production API/worker image remains separate from the contributor toolchain image.

## HTTPS and normal authentication

Exactly these authorities are registered:

- `https://account.erpx.localhost:3443`
- `https://acme.erpx.localhost:3443`
- `https://northstar.erpx.localhost:3443`
- `https://erpx-internal.erpx.localhost:3443`

Ensure these exact names resolve to127.0.0.1 on the host. Browsers commonly resolve `.localhost` themselves; other clients may require explicit resolver entries. Do not replace the names with arbitrary suffixes or omit3443. Only Acme Service is qualified here; the Northstar/Internal names are exact profile registrations, not accepted business or SSO journeys for those Holdings. The Caddy local CA is unique to the development project:

```sh
scripts/local/dev-container certificate > erpx-contributor-root.crt
```

Review and trust that local CA through the host/browser's certificate settings before signing in. Keep its trust restricted to your local development usage, and remove it when retiring the project. Trust installation is an explicit host action; scripts do not modify the host trust store. The automated browser proof pins only the four generated certificate keys and verifies each hostname against the project CA; it does not disable TLS verification generally.

Open Acme, use the normal Account sign-in and choose `acme-co` plus an explicit role. Fictional maker/reviewer credentials are in the container's root-private `.runtime/local/config/dev-container-credentials.json`; view them locally through the attached editor/terminal and do not paste them into issues or logs. The maker is **Synthetic Count maker** and the independent reviewer is **Synthetic Count reviewer**. Initial native credentials are converted to Argon2id only for the two exact fictional accounts; changed credentials are not silently overwritten on restart.

Caddy forwards the enumerated native authentication/device-session and SSO paths to Rust. Device initialization, saved-account operations and sign-in share the same native cookie authority. Unsupported methods and foreign hosts are rejected by the private API. Other requests go to the Next gateway, which retains the full external HTTPS authority. Incoming proxy metadata is stripped at the private Rust boundary. Secure/HttpOnly cookies and normal authorization remain in effect.

## Editor and CLI lifecycle

Export the same workspace key and source ref in the environment that starts your editor. Open this repository with Dev Containers support. `initializeCommand` runs `populate` on the host; `postCreateCommand` runs `bootstrap` and `postStartCommand` runs `start` inside the workspace. The CLI and editor call the same scripts. The editor workspace is the Linux source volume, not the operator checkout. Closing the editor leaves the explicitly keyed project running (`shutdownAction: none`).

```sh
scripts/local/dev-container bootstrap       # repeat migration/verification, preserve data
scripts/local/dev-container start --app=service
scripts/local/dev-container restart-native  # rebuild and gracefully replace owned Rust processes
scripts/local/dev-container down            # stop this project, retain volumes
scripts/local/dev-container up --app=service
```

The qualified Account/Apps/Service trio explicitly uses Webpack in the trusted contributor profile. Public profiles and other applications retain their configured bundler. The pinned Turbopack runtime repeatedly emitted `restart` updates after a preserved-cache container restart; CDP proved autonomous hard navigation while server PIDs and correctly namespaced sockets remained stable. Webpack is the supported Next development alternative, with HMR and filesystem caching retained. Remove this scoped choice only after a pinned Turbopack candidate passes the same source-edit and restart gates.

The three supported apps have distinct randomly generated32-byte Server Actions encryption keys in root-private `next-actions-<app>.key` files under the keyed config directory. The gateway injects only the matching key into each app process. Keys persist through restart and rotate with project reset; scripts never import operator keys or print their values. The pinned Next runtime treats Docker key storage as ephemeral, and its Webpack cache version includes that key. Persisting these development keys prevents a new random salt from invalidating every restart cache. This cache guarantee covers Account/Apps/Service only. Existing app logs expose bounded cached/built module counts, not secret values or module source.

Next source edits use HMR without rebuilding an image. Rust uses incremental Cargo caches; use `restart-native` after Rust source changes. Startup refuses to reuse a different binary under an old process lease. Native process ownership includes boot ID, PID namespace, namespace init start time, process start time, UID, command and working directory. It never terminates an unproven process on an occupied port.

An interrupted launcher can leave an unrecorded process or an in-namespace lock. Use explicit recovery instead of deleting locks or killing guessed PIDs:

```sh
scripts/local/dev-container recover --app=service
```

Recovery verifies the exact keyed secret lease and replaces only that Compose project's container namespaces. Source, databases and caches remain. Bootstrap/start locks live in container-private `/run`, so namespace replacement clears stale locks without age/PID guesses. A partial database rename resumes from a private write-ahead manifest with the same tokens; active clients block rename before any change and are never terminated.

Only deliberate disposal removes data:

```sh
scripts/local/dev-container reset --confirm="$ERPX_DEV_WORKSPACE_KEY"
```

Reset verifies ownership and removes this project's volumes only. It is not a migration, operator cleanup or production-data command.

## Persistence and runtime ownership

Linux source, toolchain-keyed `node_modules`, Cargo target/registry/Git and Bun caches use named volumes. Next `.next` caches stay under the persistent source volume. Choose a new key when changing architecture/toolchains or an incompatible dependency environment. `down/up` and `recover` preserve these volumes; `reset` removes them.

The isolated bootstrap grants no access to operator databases. Eight generated databases become exact Rust development leases, preserving current migration checksums, roles and seed receipts. The leased database guard remains unchanged; the shared namespace supplies its exact loopback endpoint. Holding and HoldingRead are distinct and the native worker projects real Count events into HoldingRead.

Rust is authoritative for the qualified Count commands, reads/SSR and projection job. The removed TypeScript Count implementation is not restored or packaged as a fallback. Next rendering, mixed-owner Inventory lists/options and named shared Finance/Identity/Files readers and Work Projection workspace preferences remain transitional dependencies, as recorded in [the Count boundary](rust-service-count-delivery.md). The legacy jobs process remains for unrelated jobs and excludes the native Count subset. These retained consumers must migrate and pass their own gates before their shared implementation is removed. Opening another app does not qualify its Rust migration or this container profile's onboarding support.

`/api/health` is liveness. The global Rust `/api/ready` intentionally remains503 while the full migration is incomplete; a running process is not evidence of a usable Count. Qualification requires a normal browser command and matching native worker receipt/projection.

This project publishes only local3443. It does not replace the operator gateway3100, edit Cloudflared routes, or change staging/private production. Public development hosts continue through the existing native Tunnel. Container traffic must be qualified separately before any future Tunnel cutover.

## Verification boundary

The source-bound evidence records fresh volumes followed by bounded fixes and affected-step retries, migration/seed, actual normal authentication/selected Service startup, a Count command and worker projection, reversible Next source-edit HMR, Rust cold/warm/edit/restore rebuilds, and restart persistence. Qualification failures are retained and corrected on the same case; failed attempts are not represented as passes. The recovery fixture separately exercises actual PostgreSQL interruption/resume, all24 runtime-role connections, active-client denial and partition-aware FK checks that retain real-orphan and disabled-trigger detection. See the [source-bound evidence record](../../testing/evidence/2026-10-30-service-count-devcontainer.json) and [delivery PR #7](https://github.com/fluke-jesadakorn/erpx/pull/7) for exact results, measured timings, retained failed attempts and diagnostic limitations. The fresh official CLI case passed from published commit `c6764f1fe877338b5bb3d2185658d9e8fed27af2` without overlays. Fresh Company/role selection has zero page errors but retains a React pre-mount warning and two development-resource404 console diagnostics; these remain wider UI backlog. The separate mounted HMR/Count readback run has zero page and console errors. Stale-lock injection/recover is qualified; a full SIGKILL fault matrix and editor GUI attachment are not claimed.
