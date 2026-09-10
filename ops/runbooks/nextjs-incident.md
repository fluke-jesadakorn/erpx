# ERPX production incident handling

Runtime: [ADR-024](../../docs/adr/ADR-024-local-postgresql-runtime.md), [operator commands](../macos/README.md).

1. Check local `http://127.0.0.1:3101/api/health` and `/api/ready`, then the same endpoints at the configured HTTPS origin. Readiness checks PostgreSQL and a recent jobs heartbeat, and returns 503 during maintenance.
2. Inspect only `system/com.erpx.postgres`, `system/com.erpx.web`, `system/com.erpx.jobs` and `system/com.erpx.tunnel` with `sudo launchctl print`. Run the operator doctor for per-Holding pending and dead-letter counts. Read bounded ERPX logs without logging secrets or document contents.
3. For a local healthy server with an unavailable public hostname, check the dedicated `erpx-local` connector, exact hostname ingress and proxied DNS. Do not alter `epsx-dev` or `n8n`.
4. For authentication failures, check the single `ERPX_PUBLIC_ORIGIN`, Resend verified sender/delivery, Better Auth secure cookies, verification state and database-backed rate limiting. A domain change requires a fresh login. Do not bypass email verification to grant administrator access.
5. Correct the cause and restart only the affected ERPX service with `sudo launchctl kickstart -k system/com.erpx.<service>`. Retry reviewed outbox events without changing event IDs or receipts. Use the [recovery runbook](nextjs-recovery.md) for database/files restore or release rollback.

Build gates are Bun lint, TypeScript, native PostgreSQL integration and the isolated standalone release build. Preserve aggregate audit records and concise operational evidence with UTC timestamps and release IDs. Never restart or stop reserved-port projects or the existing PostgreSQL 5432 cluster.
