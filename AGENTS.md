# AGENTS.md

ERPX is a Rust/Dioxus workspace rooted at this directory.

## Tooling

Use Cargo with the pinned Rust toolchain. JavaScript package managers and
Next.js are not part of this repository.

## Verification

Keep Dioxus hot reload running while editing UI Rust/RSX. Do not run Cargo tests
after every UI edit. Use the quick affected lane at a small checkpoint:

```bash
cargo xtask check-affected --dry-run
cargo xtask check-affected
```

It format-checks changed Rust files and runs owning-package `cargo check`
targets without linking tests, scanning architecture, or checking reverse
dependencies. Finish a related change set with the thorough affected lane:

```bash
cargo xtask test-affected --dry-run
cargo xtask test-affected
```

The thorough selector formats changed Rust files, tests changed package targets,
compile-checks reverse dependencies, and runs mapped domain harnesses. Use
`cargo check-fast` and `cargo test-fast` when deliberately escalating to the
whole Web workspace. Run `cargo xtask test <harness> [filter]` for a specific
domain when needed. Harness names are documented in `README.md`. Before handoff,
run the complete serialized gate once:

```bash
cargo xtask verify
```

The complete verifier includes formatting, warnings-denied workspace Clippy,
the full workspace test suite with default Web features, Web/Server Console
checks, a Dioxus Web build, SQLx acceptance, and runtime smoke checks. Do not
duplicate those commands before it or substitute a production web framework
build. Desktop/Mobile certification is deferred to the manual
`cargo xtask verify-platforms` command and must not enter the default workflow.

## Development

Run `cargo xtask dev --detach` once and keep Dioxus hot reload active. Re-run the
same command after backend edits for a selective warm restart; use
`cargo xtask dev --detach --fresh` only for a full non-destructive runtime
reconcile. Keep the ERPX development server running on port 3004 when handing
work back. Use `cargo xtask status` and `cargo xtask stop`; these commands manage
only recorded ERPX child processes.

## Runtime safety

Keep `.env`, `.env.local`, `.sqlx/`, `target/`, and `.runtime/` outside Git.
PostgreSQL, MinIO, and Ollama are host-native. Local credentials and TLS are
managed by `cargo xtask reconfigure-local`; retained databases and objects must
not be deleted during reconfiguration.

`cargo xtask reset-dev` is destructive and must retain its loopback endpoint,
schema/bucket allowlist, target printout, and literal confirmation fences.

## Structure

Applications live under `apps/`, shared crates under `crates/`, migrations
under `crates/migration/`, fixtures under `fixtures/`, architecture decisions
under `adr/`, retained ERPX evidence under `audits/`, and development
orchestration under `xtask/`.
