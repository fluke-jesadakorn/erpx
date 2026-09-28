# ERPX on this Mac

The active deployment follows ADR-024 and ADR-032: Node 24.20.0, Bun 1.2.17, shared PostgreSQL 18.6 and a dedicated Cloudflare Tunnel. Historical Wrangler/OpenNext files and `scripts/legacy-cloudflare/` are references, not deployment commands.

## Layout and ports

`/Library/Application Support/ERPX` contains private production web/jobs on loopback `3101`. `/Library/Application Support/ERPX-Staging` contains staging web/jobs on `3103`, with its own `_erpx_staging` account, auth secret and files. Both use the existing PostgreSQL 18.6 data directory on `127.0.0.1:5432`, beside development on `3100`; each has separate database names, roles and manifests. The old `5433`/`5435` clusters are disabled but their data directories remain for rollback. `com.erpx.shared-postgres` starts the shared cluster at boot. Public homepages are `dev-erpx.jesadakorn.com` and `erpx.jesadakorn.com`; their consoles are `console-dev-erpx.jesadakorn.com` and `console-erpx.jesadakorn.com`. Ports 3000, 3001 and 8080 remain reserved. See [postgres-consolidation.md](postgres-consolidation.md) for cutover and rollback details.

## Build and initial install

1. `bun install --frozen-lockfile`, then `bun run lint`, `bun run typecheck`, `bun run test:integration`, `bun run deploy:build`. Integration tests use their own random `erpx_test_*` databases on loopback PostgreSQL at 5432 under the guarded `erpx_test_admin` role. The pinned test runner loads only `ERPX_TEST_ADMIN_URL` from `.env.test.local`, `.env.test`, or `.env.local` when not explicitly set. They never target production databases.
2. `python3 ops/macos/bundle-runtime.py` copies the pinned Node/PostgreSQL/cloudflared runtimes into `.runtime/deployment/runtime`, relocates Homebrew dylibs and verifies executables. This payload is independent of later Homebrew formula upgrades.
3. Before enabling canonical attachments, install the exact ClamAV 1.5.4, libheif 1.23.5 and Poppler 26.09.0 packages listed in `clamav/runtime-versions.json`, then run `sudo /usr/bin/python3 ops/macos/install-attachment-runtime.py`. The installer does not invoke Homebrew. It creates `com.erpx.clamd` and `com.erpx.freshclam`, exposes only a mode-0660 local Unix socket to ERPX service accounts, refreshes signatures hourly, and prints the non-secret environment paths to copy into each private environment file. Follow [attachment-rollout.md](attachment-rollout.md) for the ordered migration, online backfill, frozen final reconciliation, one-time global gate opening, verification and non-destructive rollback. The release bundles `tools/reconcile-attachments.mjs`; do not depend on a mutable source checkout for an installed environment.
4. Prepare private `.runtime/deployment/production.env` with a strong Better Auth secret, AES key ring, canonical `ERPX_PUBLIC_ORIGIN`, Resend key and verified sender. Keep `ERPX_PUBLIC_BETA=false` until email/HTTPS acceptance. Store the dedicated remotely managed Tunnel token in `.runtime/deployment/tunnel.token` (0600).
5. **Legacy isolated-cluster installs only:** `install.py` initializes a separate cluster on `5433`; do not run it to install or upgrade the current shared-cluster deployment. The historical command was:

   ```sh
   sudo /usr/bin/python3 ops/macos/install.py --release .runtime/releases/latest --payload .runtime/deployment
   ```

   The current installation was migrated in place. New installations must provision only scoped roles/databases on `5432` and reuse the shared launchd service; update the legacy installer before using it for that purpose.
6. Check `curl -fsS http://127.0.0.1:3101/api/health` and `/api/ready`, then `sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' doctor`. `/api/health` and doctor report the clamd engine, signature timestamp and HEIF/PDF tool readiness. Health fails without them even while the UI gate is closed because compatibility uploads use the same scanner; final reconciliation receipts become mandatory when the gate opens. `/ready` requires a jobs heartbeat newer than 45 seconds and no maintenance flag. Doctor reports pending/dead-letter totals per Holding without document contents or credentials.
7. Keep production private on `3101`. The public `erpx.jesadakorn.com` host is reserved for staging on `3103`; development uses `dev-erpx.jesadakorn.com` on `3100`. The existing `erpx-local` Tunnel and DNS records are managed in Cloudflare Dashboard; do not modify epsx-dev or n8n.
8. Verify real Resend delivery, HTTPS redirects, auth reset/verification links, secure cookies, all static/PDF assets and registration in an isolated acceptance database before enabling signup in the installed env file and restarting only ERPX web/jobs. Sign up as the agreed admin, verify the email, then run `sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' bootstrap-admin`. This command refuses an absent/unverified account and all other emails.

## Services and secrets

Active labels are `com.erpx.shared-postgres`, `com.erpx.web`, `com.erpx.jobs`, `com.erpx.tunnel`, `com.erpx.backup`, `com.erpx.staging.web`, `com.erpx.staging.jobs`, and `com.erpx.staging.backup`. The old `com.erpx.postgres` and `com.erpx.staging.postgres` labels are disabled. Use `sudo launchctl print system/com.erpx.web` to inspect one service; `sudo launchctl kickstart -k system/com.erpx.web` restarts only production web. Jobs recovers outbox leases after process loss and polls committed events once per second. Schedules run every two minutes, leases last 90 seconds with renewal, and an initial attempt plus five retries precedes dead-letter. Inspect failures with doctor; only replay after correcting the cause. Clear `dead_letter_at`, reset `attempts`/`next_attempt_at` for the specific reviewed event via the operator database account; never reset event IDs or projection receipts.

For a planned PostgreSQL restart, pause development and both ERPX web/jobs environments, then use `sudo launchctl kill SIGINT system/com.erpx.shared-postgres` and wait for its new PID/readiness before restarting web/jobs. This shared service also serves other local projects; coordinate their downtime. SIGINT is PostgreSQL fast shutdown; `KeepAlive` starts it again.

Daemon environments explicitly set `LC_ALL=C` and `LANG=C`; an unset locale can make PostgreSQL abort with “postmaster became multithreaded during startup” on macOS. Code symlinks (`current` and the Next cache link) explicitly allow traversal with mode 0755; macOS enforces symlink permissions even when the destination directory is accessible. Keep secret/data permissions private when repairing a code link.

`config/production.env` and `tunnel.token` are root:_erpx 0640; `admin.json` is root-only 0600. The environment administrator credential is used only by provisioning/backup/restore. Per-database app/jobs manifests and, where present, a migration manifest live under `data/config/`. App/jobs roles cannot write read-model tables or migrate schema; migrated roles are restricted to their environment in `pg_hba.conf` and database grants. Limit pools to one per Holding database and a small control pool. The shared cluster currently allows 180 connections; do not raise it without measured demand. Back up all secrets with the data to retain authentication and AI credential decryption.

Node web/jobs/Tunnel logs rotate at 5 MiB with seven archives. PostgreSQL rotates daily across seven weekday files and suppresses SQL/parameters in errors. Startup logs rotate through newsyslog. No HTTP request or document-body logger is enabled. Check Mac disk space; this beta uses a single disk.

## Backup, restore and release rollback

Run a backup now with:

```sh
sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' backup
sudo /usr/bin/python3 '/Library/Application Support/ERPX-Staging/ops/erpxctl.py' --home '/Library/Application Support/ERPX-Staging' backup
```

Each environment has its own operator lock, maintenance flag, scoped database manifests, files and backup directory. Production runs daily at 03:15; staging runs daily at 03:45. Maintenance denies requests, only that environment's web/jobs stop, and the command checks that its database sessions have exited before dumping its databases and copying private files, manifests and secrets. When MinIO is configured, the pinned release tool also copies and checksums every object under that environment's `environments/<environment>/` prefix in the existing bucket, including Holding and platform billing objects. It rejects unexpected keys and stops backup if any object cannot be copied. Dumps use PostgreSQL custom format. A snapshot becomes visible only after all files and SHA-256 checksums are written; incomplete `.partial` directories are not restorable backups. Services restart on backup failure. Seven complete local snapshots are retained, including a snapshot selected for restore.

For a filesystem-to-MinIO cutover, first configure an existing reachable instance and bucket with `ERPX_MINIO_ENDPOINT`, `ERPX_MINIO_BUCKET`, `ERPX_MINIO_ACCESS_KEY`, `ERPX_MINIO_SECRET_KEY`, and `ERPX_MINIO_REGION` in that environment's private env file. Install the release and updated backup operator before enabling MinIO for staging or production. The operator pauses that environment's web and jobs writers, runs the bundled migration tool, then resumes them:

```sh
sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' migrate-files plan
sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' --expect-count <count-from-plan> migrate-files apply
sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' --expect-count <count-from-plan> migrate-files verify
```

For staging, add `--home '/Library/Application Support/ERPX-Staging'` to each command. The migration checks every source and destination checksum, database file references, and object count; it keeps local originals. Verify a complete MinIO-inclusive snapshot and restore rehearsal before removing local copies. The local development variant is `bun scripts/local/migrate-files-to-minio.ts` with explicit `--data-dir`, `--environment local` and the same plan/apply/verify modes.

### Encrypted off-machine copy

The operator can stream each finished snapshot through `age` into an encrypted archive on a **separately mounted filesystem**. Its private decryption identity must be created and kept on the recovery host; install only the corresponding `age1...` public recipient on this Mac. The production operator rejects a destination on the same filesystem as the ERPX home. The archive includes all configured database dumps, private files, MinIO object snapshots when configured, manifests, secrets, and the release identity; a SHA-256 sidecar detects transfer damage before decryption. No plaintext tar file is created. Keep the destination mounted, access-controlled, and independently backed up. Never put the identity in `production.env`, the ERPX home, Git, or the offsite mount.

After installing the updated operator files in the root-owned `ops/` directory during a reviewed release, create root-only `/Library/Application Support/ERPX/config/offsite.json` with the mounted directory and public recipient. This fictional example is a template, not a usable recipient:

```json
{"destination":"/Volumes/ERPX-Recovery/erpx","recipient":"age1replacewithrecoveryhostpublicrecipient"}
```

Set the config file to mode `0600`. The existing backup service then exports each snapshot after restarting web/jobs; export failure makes the backup command fail visibly while retaining its complete local snapshot. To export an older snapshot manually:

```sh
sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' --destination '/Volumes/ERPX-Recovery/erpx' --recipient 'age1...' export '/Library/Application Support/ERPX/backups/<timestamp>'
```

On a separate recovery host, install the matching ERPX release and provision the same database names in an isolated PostgreSQL cluster. Copy the `.tar.age` archive and its `.json` sidecar from the offsite store, then run the updated operator with the recovery host's private identity:

```sh
sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' --identity '/private/recovery/erpx-age-identity.txt' import '/private/recovery/<timestamp>.tar.age'
sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' restore '/Library/Application Support/ERPX/backups/<timestamp>'
```

Record the archive timestamp, completed export time, import/restore start and finish, database row checks, private file checks, app-secret checks, and an authenticated smoke test. The isolated automated rehearsal covers encrypted export, wrong-key and tamper rejection, import, and complete restore. It does **not** establish off-machine durability or the target of at most one hour of data loss and four hours to restore. The installed backup schedule is still daily at 03:15; select and prove an online backup/WAL strategy and separately mounted destination before the pilot production gate. Do not claim the RPO/RTO from a local or simulated restore.

After installing the updated operator, run `sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' recovery-status` for read-only recovery facts. It checks private offsite configuration, a separate destination device, the latest archive's sidecar, size and ciphertext SHA-256, and whether that archive is less than one hour old. `rpoProven` and `rtoProven` remain false until a real online/WAL design and separate-host timed restore are accepted; a fresh archive alone does not establish either target. The command prints no recipient, destination path, or credentials.

Restore with `... erpxctl.py restore '/Library/Application Support/ERPX/backups/<timestamp>'`. It verifies every checksum, the database set and any MinIO snapshot before creating a current-state backup. A MinIO snapshot requires an installed release with `tools/minio-snapshot.mjs` and a configured bucket with the same name; its environment prefix must match the selected operator home. Restore writes only the snapshot's objects under that exact prefix, leaving other environments and additional objects untouched. Review additional objects separately after a restore, especially legally preserved documents. The command then restores databases and local files, restores app secrets, and activates the matching release. Existing cluster login credentials are retained. If restore fails midway, maintenance stays active and web/jobs stay stopped: inspect logs and rerun a verified restore after resolving the failure; remove the maintenance marker only after recovery. For a new cluster, first install the matching release and provision the same database/role names, then restore. Test this on a separate recovery host/cluster before a real disaster.

For a code rollback with unchanged schema, use `... erpxctl.py rollback <release-id>`. It refuses different schema hashes. A schema change requires a matching backup restore; never point an older release at an incompatible schema. Keep the releases referenced by retained backups.

## Upgrade

Build and validate a new isolated release. Create a backup, enter maintenance, and stop only ERPX web/jobs. Copy the release into root-owned `releases/`; do not edit `current` in place. Run its provision tool with the root-only admin credentials to apply append-only checksum-verified migrations and role grants. Keep `_erpx` ownership on data and symlink `.next/cache` to `data/next-cache`. Atomically replace `current`, restart ERPX jobs/web and check readiness and HTTPS. If schema is unchanged, operator rollback is available; otherwise use the pre-upgrade backup. Tunnel and PostgreSQL need no restart for an application release.

## Future domain

Add the new hostname's DNS and Tunnel ingress while the old hostname remains available. Verify the new Resend sender domain. Pause registration, change only `ERPX_PUBLIC_ORIGIN` and `RESEND_FROM` in production.env, restart web/jobs, and test verification/reset/invitation links and secure cookies at the new origin. Existing Holding IDs, database names, object keys and `/holdings/<slug>` paths do not change. Users sign in again on the new cookie origin. Remove old ingress/DNS only after the transition is accepted. Do not add automatic cross-domain session copying.

## Acceptance record

See `.scratch/local-deploy/` for this workstation's build/test logs (not committed, no credentials). Test actual service crash/restart after install without touching other projects. Schedule a full Mac reboot only when the owner is ready; do not reboot automatically. Public registration must remain closed while email verification or HTTPS acceptance is incomplete.
