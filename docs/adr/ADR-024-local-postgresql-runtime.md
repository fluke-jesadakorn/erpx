# ADR-024: Local Node.js and PostgreSQL behind Cloudflare Tunnel

> **Accepted — 2026-09-08. Supersedes the runtime and deployment portions of ADR-022.**

ERPX runs a Next.js standalone release on Node.js 24.20.0, PostgreSQL 18.6 and a private filesystem on this Mac. Bun 1.2.17 manages and builds the project. Cloudflare Tunnel `erpx-local` publishes only `erpx.jesadakorn.com` to `127.0.0.1:3101`. This supports the agreed single-machine public beta and native database transactions while accepting dependence on the Mac's uptime and disk. Workers, D1, R2 and Queues configuration remains historical reference.

The control plane and each Holding retain separate write/read databases, with separate scoped web/jobs logins. Business changes, authorization assertions, command receipts, audit and outbox commit on one leased PostgreSQL connection in a serializable transaction. Read projections remain rebuildable. Immutable filesystem revisions use atomic publication and remain behind existing authorization. A PostgreSQL outbox supplies leases, retry, replay and dead-letter state; a Node process handles work and two-minute schedules.

Production code is a separate immutable release under `/Library/Application Support/ERPX`. A hidden `_erpx` account runs four boot LaunchDaemons: PostgreSQL, web, jobs and Tunnel. A root operator backup daemon pauses web and jobs and snapshots all databases with the private files. Seven complete backups are retained on this disk; a failed restore stays in maintenance until repaired. Disk failure still requires an off-machine backup, which is outside this deployment.

The beta starts with only schema and 20 organization slots, with verified signup, resumable provisioning, and existing 20-member/5-branch/1-GiB limits. An explicit local bootstrap grants platform administration only to the configured, verified email; the first signup receives no special privilege. Public origin and sender are configuration, so moving to a purchased domain preserves Holding identities and paths, requires newly verified sender DNS, and requires users to sign in again. Live billing remains disabled.

Native PostgreSQL integration, lint, typecheck, standalone build, backup/restore and local acceptance replace the Cloudflare build/preview gates. Real email delivery and HTTPS acceptance must precede opening signup. A full Mac reboot is an operator-scheduled acceptance step.
