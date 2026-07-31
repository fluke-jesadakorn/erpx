# AGENTS.md

ERPX is a Rust/Dioxus workspace rooted at this directory.

## Tooling

Use Cargo with the pinned Rust toolchain. JavaScript package managers and
Next.js are not part of this repository.

## Verification

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline --no-fail-fast
```

Use `cargo xtask verify` for the complete serialized Dioxus and runtime smoke
suite. Do not substitute a production web framework build.

## Development

Run `cargo xtask dev --detach`. Keep the ERPX development server running on
port 3004 when handing work back. Use `cargo xtask status` and
`cargo xtask stop`; these commands manage only recorded ERPX child processes.

## Runtime safety

Keep `.env`, `.env.local`, `.sqlx/`, `target/`, and `.runtime/` outside Git.
PostgreSQL, MinIO, Ollama, and n8n are host-native. The n8n state and encryption
key under `.runtime/n8n/` must never be deleted or replaced.

`cargo xtask reset-dev` is destructive and must retain its loopback endpoint,
schema/bucket allowlist, target printout, and literal confirmation fences.

## Structure

Applications live under `apps/`, shared crates under `crates/`, migrations
under `crates/migration/`, fixtures under `fixtures/`, architecture decisions
under `adr/`, retained ERPX evidence under `audits/`, and development
orchestration under `xtask/`.
