# ADR-015: Retire n8n and unify the local Cargo/runtime boundary

**Status:** Accepted

**Decision:** ERPX no longer exposes n8n as a connector, bootstrap phase, local
runtime dependency, or rollback-preserved resource. The former protobuf enum
number `3` and name are reserved permanently. Forward migrations fail closed
when n8n appears in connector authority, projection, receipt, audit, or outbox
evidence; `Other("n8n")` is also rejected.

All workspace Cargo workflows use the root `target/`, and native development
binaries run from `target/debug`. Runtime state and Cargo artifacts are not
mixed below `.runtime/erpx`.

Local credential/TLS replacement is performed only by
`cargo xtask reconfigure-local`. The command requires a dry-run-visible target
inventory, exact loopback and fixed-root fences, inactive PITR, a matching
complete bootstrap receipt and active holding, and the literal confirmation
`RECONFIGURE_ERPX_LOCAL`. It retains all PostgreSQL databases and MinIO objects,
rotates the four managed PostgreSQL roles and MinIO root credentials, replaces
the local CA/server certificates and managed environment keys, probes the
retained resources, and rolls configuration and credentials back if a probe
fails.

**Consequences:** The current runtime manifest and service catalog
contain no n8n fingerprint. Bootstrap progress version 2 contains no n8n phase.
The one-time successful local reconfiguration removes `.runtime/n8n` and the
superseded `.runtime/erpx` contents, then performs a clean workspace rebuild.

This decision supersedes the n8n-specific portions of ADR-009 and ADR-014. It
does not rewrite historical audits, cutover records, or already-applied
migrations.
