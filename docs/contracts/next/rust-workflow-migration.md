# Native Workflow command ledger

This component supplies command receipts, fingerprints, audit and outbox effects under [ADR-052](../../adr/ADR-052-rust-backend-and-domain-aligned-frontend.md). It is not complete Workflow owner migration or an activated HTTP backend. The [owner register](../../status/rust-backend-migration.json) keeps routing, SSR, jobs and remaining owner behavior pending.

## Ownership and transaction contract

`PgCommandLedger` borrows the caller's PostgreSQL connection and requires an existing `SERIALIZABLE` transaction. It cannot open a pool connection, commit, retry or call providers. Production composition supplies mandatory current Identity authorization and owner policy through `PgCommandAuthority`, plus mandatory source locking, effects and final retention through `PgOwnerCommandEffects`. There are no permissive defaults. `CommandActor` is adapter data with no request deserialization or grant semantics.

The composition root must acquire its complete owner/Identity utility lock bundle before its first data snapshot. Taking source locks later cannot refresh an old serializable snapshot. The ledger serializes each command key with a transaction advisory lock, checks current personal authority, and compares the complete request fingerprint before selecting historical replay policy. A successful replay returns the committed result without repeating owner, audit or outbox effects; it still requires current personal authorization. An expired purchase's history purpose cannot authorize a new command.

A new command inserts its predeclared typed-owner result receipt before applying canonical owner effects. Audit and outbox records use the same connection. Owner final retention, command policy revalidation, assertion cleanup and unchanged current authority must all succeed before the enclosing unit commits. Errors must propagate to that enclosing unit; catching an error and committing partial effects is unsupported. External effects belong in durable jobs/outbox consumers.

Concurrent conflicting insertion against an older snapshot retains PostgreSQL's real `40001`; the enclosing unit must rerun the entire operation with fresh source reads. The no-op conflict clause is safe only with the enforced serializable isolation. A regression observes two actual PostgreSQL backends waiting on the command lock, then verifies `40001` and a fresh whole-unit replay with one owner effect, receipt, audit and outbox row.

## Persisted JavaScript compatibility

`CommandJson` normalizes bounded JSON using captured ECMAScript behavior rather than unordered object serialization. Input and output are limited to 2 MiB and nesting to 128. The implementation preserves insertion order, integer property enumeration, duplicate-key replacement position, IEEE-754 numbers, negative zero, nonfinite overflow normalization and escaped lone UTF-16 surrogates. The pure Rust `ryu-js` formatter supplies ECMAScript number formatting. Lone surrogate compatibility for receipt bytes does not authorize those values in a database or business DTO.

Fingerprints preserve the original literal actor field order and omitted-versus-null values. IAM metadata is retained in receipt/audit/outbox data but does not participate in the request hash. Original assignment scope is distinct from the selected Company/Branch. A saved legacy receipt may reconstruct its historical three-field work authority only after matching its operation, actor, record and current scope facts. Saved authority never replaces current Identity authorization.

The narrow Guest Account compatibility bridge retains old persisted spellings and the captured JavaScript property-lookup quirks as data only. It creates no prototypes, functions or legacy routes. Exact oracle files are explicitly excluded from the terminology guard so they can prove historical compatibility; new business vocabulary remains subject to that guard.

The source-bound development oracle imports unchanged B0 TypeScript on pinned Node 26.11.1. Its corpus contains 47 JSON normalization cases, 227 fingerprints, 54 bidirectional data conversions, 15 operation mappings and 24 historical replay cases. Generation supports `--check`; it is development verification, not a Node dependency of the native runtime. Synthetic test authority ports are labelled explicitly. Actual Legal/Identity composition has separate PostgreSQL integration tests and remains unmounted pending full migration.
