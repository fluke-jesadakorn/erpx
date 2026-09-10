# Local full-stack implementation research

> **Superseded 2026-09-06 by [ADR-022](../adr/ADR-022-cloudflare-runtime.md).** Retained as decision history; PostgreSQL, MinIO, Docker, and a persistent Bun worker are outside the active implementation.

Date checked: 2026-09-06. Status: research and proposed implementation choices, not an accepted ADR or a claim that the services have been installed or tested. Scope: the existing Next.js app, Bun, local PostgreSQL, and local MinIO uploads.

## Verified repository baseline

The repository pins Next.js `16.3.4`, React/React DOM `19.2.8`, and currently uses `next dev`, `next build`, `next start`, and ESLint scripts. No database or object-storage driver is declared. Source: [package.json](../../package.json).

The installed Next.js guides were consulted directly, as required by AGENTS.md:

- `node_modules/next/dist/docs/01-app/01-getting-started/01-installation.md`: documented Node.js minimum is 20.9.
- `node_modules/next/dist/docs/01-app/01-getting-started/15-route-handlers.md`: App Router `route.ts` supports HTTP handlers; handlers are not cached by default; `params` in the documented handler example are awaited.
- `node_modules/next/dist/docs/01-app/02-guides/server-actions.md`: action request bodies default to a 1 MB limit.
- `node_modules/next/dist/docs/01-app/02-guides/data-security.md`: perform resource authorization inside actions and centralize privileged data access in server-only modules. A page-level check does not secure its actions.

These installed guides take precedence over examples from older Next.js versions. Public counterparts: [Route Handlers](https://nextjs.org/docs/app/getting-started/route-handlers), [data security](https://nextjs.org/docs/app/guides/data-security).

## Runtime and package management

**Verified:** Bun's official Next.js guide explicitly documents running the Next development and production servers with Bun and scripts `bun --bun next dev`, `bun --bun next build`, and `bun --bun next start`. The explicit runtime flag matters; installing dependencies with Bun alone does not establish which runtime runs a CLI. [Bun Next.js guide](https://bun.sh/guides/ecosystem/nextjs), [Bun runtime](https://bun.com/docs/runtime).

**Recommendation:** adopt Bun as the package manager and intended application runtime; pin its exact version and commit its lockfile after a dependency migration check. Keep the existing Next/React pins during the infrastructure change. Update repository instructions and CI alongside package scripts, so developers do not alternate lockfiles. Do not add a separate Bun HTTP framework: Next.js already owns application routes and server execution.

**Unverified until the first spike:** the exact pinned Next.js/React/Bun combination, native dependency behavior, production build, and sustained server operation. Acceptance must exercise both `build` and `start`, database transactions, authentication cookies, and real browser uploads. Documentation establishes the supported approach, not successful execution of this repository.

## Database choice and operation

**Verified:** PostgreSQL 18.6 and 17.11 are currently supported; upstream lists support through November 2030 and November 2029 respectively. PostgreSQL recommends the current minor release for a selected major. Major upgrades require an explicit upgrade or dump/reload process. [PostgreSQL version policy](https://www.postgresql.org/support/versioning/).

**Recommendation:** use PostgreSQL 18.6 locally, pin the image digest during implementation, and use a persistent named volume. Run Next/Bun on the host initially for fast reload, with the database and storage service in Compose. Bind developer service ports to loopback. Use separate migration and application roles, health checks, explicit connection-pool limits/timeouts, checked-in SQL migrations, and idempotent seed data. Test migrations against an empty database and an existing prior schema. A volume is persistence, not a backup: define `pg_dump`/`pg_restore` operations and actually restore a copy. [PostgreSQL backup documentation](https://www.postgresql.org/docs/18/backup.html), [pg_dump](https://www.postgresql.org/docs/18/app-pgdump.html).

| Candidate | Verified capability | Recommendation for ERPX |
| --- | --- | --- |
| Drizzle + `pg` | Official PostgreSQL integration supports `node-postgres` and an explicit `Pool`. | Preferred: SQL-visible schemas and migrations suit ERP constraints, joins, transactions, and audit requirements. This is an architectural judgment, not a benchmark claim. |
| Prisma + PostgreSQL adapter | Official docs support self-hosted PostgreSQL, `@prisma/adapter-pg`, and Bun usage. | Viable if the team prefers generated-client/schema tooling. It does not require a managed Prisma database. |

Sources: [Drizzle PostgreSQL](https://orm.drizzle.team/docs/get-started-postgresql), [Prisma PostgreSQL connector](https://docs.prisma.io/docs/orm/core-concepts/supported-databases/postgresql), [Prisma Bun guide](https://www.prisma.io/docs/guides/v7/runtimes/bun). Select a stable, pinned ORM/CLI combination during the spike: the currently served Drizzle example uses `@rc`, and Prisma has version-specific guides. Do not copy a prerelease selector or mix major-version setup snippets accidentally. Prefer portable `pg` initially over introducing runtime-specific SQL calls throughout business modules.

## MinIO distribution is a material implementation decision

**Verified:** the official `minio/minio` repository is archived, marked read-only as of April 25, 2026, and explicitly says it is no longer maintained. Its README describes community distribution as source-only, with historical binaries unmaintained. It points users to AIStor Free and Enterprise. A source build is therefore not evidence of ongoing upstream maintenance. [Official MinIO repository](https://github.com/minio/minio).

**Verified:** AIStor Free's agreement allows standalone single-node use and excludes distributed clustering/high availability. Current installation documentation includes single-node single-drive and multi-drive patterns for Free. [AIStor Free agreement](https://www.min.io/legal/aistor-free-agreement), [AIStor installation](https://docs.min.io/aistor/installation/linux/install/deploy-aistor-on-ubuntu-server/).

**Recommendation:** preserve the user's requested local MinIO architecture, with an explicit choice before provisioning: a pinned community source build for isolated development with its maintenance limitation recorded, or AIStor Free after the owner chooses its licensing terms. Do not present an unpinned `minio/minio:latest` Compose entry as a maintained default. Record the selected edition, source/image digest, CPU architecture, initialization process, and upgrade owner. Keep application storage behind an S3 adapter so this packaging decision does not affect domain services.

## Proposed upload contract

**Verified S3 semantics:** presigned URLs grant time-limited bearer access, may be reused until expiry, and uploading to an existing key replaces the object. [Amazon S3 presigned URLs](https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-presigned-url.html). MinIO compatibility with the selected SDK version and exact checksum/signing options still needs an integration test; S3 compatibility is not proof of every AWS extension.

**Proposed flow:**

1. Authenticate and authorize the target business record. Validate allowed extension/type, declared size, and quota. Create a pending database attachment with a server-generated key.
2. Issue a short-lived presigned upload for a temporary key in a private bucket. Keep credentials server-side. Configure CORS for the actual local application origin.
3. Browser uploads bytes directly to the storage endpoint. Sign a browser-reachable URL; a Compose hostname such as `minio` is not normally resolvable by the host browser. Do not rewrite the URL host after signing.
4. Finalization checks storage metadata and actual content as appropriate, then copies/promotes to a new server-owned final key that the upload credential cannot overwrite. A database transaction marks the attachment ready and links it to the authorized record. Duplicate finalization must return the same result.
5. Only ready attachments are readable. Authorize each download before generating a short-lived URL. Clean abandoned temporary objects and failed promotions with a retryable reconciliation job.

This design avoids sending file bytes through Server Actions and their default size cap. Store metadata, ownership, relation, size, digest, status, and audit events in PostgreSQL; store bytes in MinIO. Database and object storage cannot share an ordinary SQL transaction, so explicitly test failure between promotion and database commit. Retention/deletion and backup must cover both stores, including a coordinated restore drill.

## First implementation gate

Deliver a reproducible local stack and one authenticated record with an attachment before expanding screens. Verify install from the committed lockfile; lint/typecheck/build/start under the selected runtime; migrate/seed/restart persistence; rollback of a failing SQL transaction; allowed and denied object access; expired/reused upload URL behavior; invalid/oversized file rejection; interrupted upload cleanup; and restoration of both database metadata and file bytes. Runtime version, storage edition, file-size policy, and expected local-only versus LAN access remain implementation choices, not facts established by this research.
