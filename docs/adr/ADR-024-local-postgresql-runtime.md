# ADR-024: Local Node.js and PostgreSQL behind Cloudflare Tunnel

> **Accepted — 2026-09-08. Supersedes the runtime and deployment portions of ADR-022.**

> **Topology update — 2026-09-24:** ADR-032 supersedes this decision's dedicated production PostgreSQL daemon and public hostname mapping. The Node/PostgreSQL runtime and transactional model remain in effect.

> **Storage update — 2026-09-27:** ADR-035 supersedes the private-filesystem choice for durable uploaded and generated file content in environments configured with MinIO. PostgreSQL still owns business configuration and file metadata.

ERPX runs a Next.js standalone release with PostgreSQL 18.6 and a private filesystem on this Mac. Development and new standalone releases target Node.js 26.11.1 and Bun 1.4.3; the installed private production service remains on Node.js 24.20.0 until a separately reviewed deployment. Cloudflare Tunnel publishes the development console at `console-dev-erpx.jesadakorn.com` to `127.0.0.1:3100`; installed production stays private on loopback `3101`. Staging routing is a separate explicitly authorized operation. This supports the agreed single-machine public beta and native database transactions while accepting dependence on the Mac's uptime and disk. Workers, D1, R2 and Queues configuration remains historical reference.

> **Toolchain update — 2026-10-12:** Development and new standalone releases target Node.js 26.11.1, Bun 1.4.3, Next.js 16.4.0 and React 19.3.0. The installed private production release remains on Node.js 24.20.0 until a separately reviewed deployment; staging is unchanged.

The control plane and each Holding retain separate write/read databases, with separate scoped web/jobs logins. Business changes, authorization assertions, command receipts, audit and outbox commit on one leased PostgreSQL connection in a serializable transaction. Read projections remain rebuildable. Immutable filesystem revisions use atomic publication and remain behind existing authorization. A PostgreSQL outbox supplies leases, retry, replay and dead-letter state; a Node process handles work and two-minute schedules.

Production code is a separate immutable release under `/Library/Application Support/ERPX`. The installed `_erpx` web/jobs services use the shared PostgreSQL daemon on 5432 (ADR-032); the dedicated old PostgreSQL daemon is retired. A root operator backup daemon pauses web and jobs and snapshots all databases with the private files. Seven complete backups are retained on this disk; a failed restore stays in maintenance until repaired. Disk failure requires separately durable off-machine backups and a timed restore; the operations contract provides encrypted export/restore tooling, whose local rehearsal does not establish off-machine RPO/RTO.

The beta starts with only schema and 20 organization slots, with verified signup, resumable provisioning, and existing 20-member/5-branch/1-GiB limits. An explicit local bootstrap grants platform administration only to the configured, verified email; the first signup receives no special privilege. Public origin and sender are configuration, so moving to a purchased domain preserves Holding identities and paths, requires newly verified sender DNS, and requires users to sign in again. Live billing remains disabled.

Native PostgreSQL integration, lint, typecheck, standalone build, backup/restore and local acceptance replace the Cloudflare build/preview gates. Real email delivery and HTTPS acceptance must precede opening signup. A full Mac reboot is an operator-scheduled acceptance step.
