# ERPX Docs Archive

This directory archives Rust-era contracts and schemas before the Next.js cutover.

- `contracts/proto/` — 37 `*.proto` files from `crates/proto/proto/erpx/**` plus descriptor hashes. Legacy source: `f97b74d6 backup/rust-2026-09-02`.
- `contracts/descriptor/` — `erpx_descriptor_set.sha256` and prior descriptor.
- `schema/migrations*` — SQLx embedded migrations for control-write/read and holding-write/read (generation-v2, 28 baseline files) from `crates/migration/`.

These are reference-only. The Next.js rebuild will derive new migrations (e.g., Prisma/Drizzle) and API contracts (REST/tRPC) from `Plan/` + `adr/` + `CONTEXT.md`.

Recovery: full Rust source is on branch `backup/rust-2026-09-02` tag `rust-archive-2026-09-02`, plus baseline `cc0887d` per `CUTOVER.md`.
