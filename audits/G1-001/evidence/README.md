# G1-001 evidence

The SQLx control-plane integration evidence is maintained in
`erpx/crates/infrastructure/tests/sqlx_control_plane.rs`.

Run it against two disposable PostgreSQL databases:

```text
ERPX_TEST_CONTROL_WRITE_DATABASE_URL=...
ERPX_TEST_CONTROL_READ_DATABASE_URL=...
cargo test -p erpx-infrastructure --test sqlx_control_plane -- --ignored --nocapture
```

The test verifies embedded migrations, atomic aggregate snapshot/outbox persistence,
optimistic concurrency, idempotency replay/completion, aggregate-ordered claims, and
read-model projection. The disposable databases must be removed after the run.
