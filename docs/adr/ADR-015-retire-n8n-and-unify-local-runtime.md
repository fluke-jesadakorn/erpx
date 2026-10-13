# ADR-015: Retire n8n as an ERPX runtime dependency

Accepted in the Rust era; the retirement boundary remains in effect. The archived protobuf number/name are permanently reserved. n8n is not an ERPX connector, bootstrap requirement or rollback-preserved ERPX resource. This decision does not authorize stopping or changing an unrelated host service.

The former Cargo `target/` layout and `cargo xtask reconfigure-local` belonged to the archived runtime. Current runtime/configuration/credential procedures are in [ADR-024](ADR-024-local-postgresql-runtime.md) and [operations](../../ops/macos/README.md). Historical database/object ownership and the bounded development reset remain in [CUTOVER](../../README.md#recovery); documentation maintenance never repeats that reset.
