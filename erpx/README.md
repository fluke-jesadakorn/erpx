# ERPX

ERPX is the Rust/Dioxus successor to the Folio pilot. Folio stays runnable while
bounded contexts are moved behind Tonic services and separate CQRS persistence.

## Workspace

- `crates/domain`: pure customer, support, expiring-access, commercial, and CQRS rules.
- `crates/application`: use cases and ports; no database or transport dependencies.
- `crates/infrastructure`: PostgreSQL read/write pools, in-memory development adapters,
  outbox/projector support, and tenant-store routing.
- `crates/proto`: versioned protobuf contracts and generated Tonic clients/servers.
- `apps/server`: modular-monolith Tonic host.
- `apps/console`: shared Dioxus Customer Operations console. The default feature is
  web; `desktop` and `mobile` use the same component tree.

## Commands

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p erpx-console --no-default-features --features web
cargo check -p erpx-console --no-default-features --features desktop
```

The server reads `ERPX_CONTROL_WRITE_DATABASE_URL` and
`ERPX_CONTROL_READ_DATABASE_URL`. Tenant databases are registered by opaque tenant
ID; customer-facing code never receives database credentials.
