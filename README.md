# ERPX

ERPX is a Rust workspace with a Dioxus console, Tonic APIs, durable workers,
SQLx persistence, native identity, and shared desktop/mobile UI. It replaces
the former development application completely and serves the console at
<http://localhost:3004>.

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
cargo xtask dev --detach  # migrate, seed, and start API/worker/Dioxus
cargo xtask status        # report only recorded ERPX child processes
cargo xtask stop          # stop only recorded ERPX child processes
cargo xtask seed-demo     # idempotent development fixture
cargo xtask verify        # serialized Rust, Dioxus, scan, and smoke gates
```

Runtime logs and PID records are under ignored `.runtime/erpx/`. The Tonic API
listens on `127.0.0.1:50051`, HTTP auth/health on `127.0.0.1:50052`, and the
Dioxus console on `0.0.0.0:3004`.

## Workspace

- `apps/server`: Tonic services and HTTP auth/health endpoints
- `apps/worker`: durable projectors, schedulers, and reconciliation workers
- `apps/console`: Dioxus web/desktop/mobile console
- `apps/migrator`: generic legacy reconciliation and rollback tooling
- `crates/domain`, `crates/application`: product contracts and use cases
- `crates/infrastructure`: SQLx repositories and provider adapters
- `crates/identity`: native identity, MFA, and directory connectors
- `crates/migration`: control/tenant write/read migrations
- `crates/proto`: public gRPC contracts and descriptor baseline
- `crates/ui`: shared product UI
- `audits`: retained ERPX implementation and acceptance evidence
- `xtask`: bounded development lifecycle and verification

## Verification

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline --no-fail-fast
cargo xtask verify
```

Non-loopback database, object-store, provider, and directory endpoints must use
certificate-validating TLS. Production activation and external directory
certification are deliberately outside this development cutover.
