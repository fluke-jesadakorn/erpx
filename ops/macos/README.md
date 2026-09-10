# ERPX on this Mac

The active deployment is ADR-024: Node 24.20.0, Bun 1.2.17, PostgreSQL 18.6 and a dedicated Cloudflare Tunnel. Historical Wrangler/OpenNext files and `scripts/legacy-cloudflare/` are references, not deployment commands.

## Layout and ports

`/Library/Application Support/ERPX` contains `releases/<timestamp>`, the atomic `current` symlink, pinned `runtime/`, root-managed private `config/`, service-owned `data/`, `logs/` and root-private `backups/`. Code is root-owned; `_erpx` can write only data/cache/logs. Secrets never enter a release or Git. PostgreSQL listens only at 127.0.0.1:5433; the web listens only at 127.0.0.1:3101. Existing development 3100, PostgreSQL 5432 and other tunnels are independent. Ports 3000, 3001 and 8080 remain reserved.

## Build and initial install

1. `bun install --frozen-lockfile`, then `bun run lint`, `bun run typecheck`, `bun run test:integration`, `bun run deploy:build`. Integration tests use their own random `erpx_test_*` databases on a separate loopback PostgreSQL server at 5434; set `ERPX_TEST_ADMIN_URL` for that cluster. They never target production databases.
2. `python3 ops/macos/bundle-runtime.py` copies the pinned Node/PostgreSQL/cloudflared runtimes into `.runtime/deployment/runtime`, relocates Homebrew dylibs and verifies executables. This payload is independent of later Homebrew formula upgrades.
3. Prepare private `.runtime/deployment/production.env` with a strong Better Auth secret, AES key ring, canonical `ERPX_PUBLIC_ORIGIN`, Resend key and verified sender. Keep `ERPX_PUBLIC_BETA=false` until email/HTTPS acceptance. Store the dedicated remotely managed Tunnel token in `.runtime/deployment/tunnel.token` (0600).
4. Inspect `install.py`. After local acceptance and confirming ports 3101/5433 are free, run:

   ```sh
   sudo /usr/bin/python3 ops/macos/install.py --release .runtime/releases/latest --payload .runtime/deployment
   ```

   The installer creates `_erpx`, installs root-owned releases and four LaunchDaemons plus a daily 03:15 backup. It does not stop another project's process. An existing installation is rejected. After inspecting an interrupted initial install that has not registered web/jobs/Tunnel, rerun the same command with `--resume`; it preserves the cluster, secrets and release and reloads only ERPX PostgreSQL before continuing provisioning. A different release or an installation with application services already registered is rejected. LaunchDaemons start at boot without a user login. FileVault boot unlock can delay services if enabled later.
5. Check `curl -fsS http://127.0.0.1:3101/api/health` and `/api/ready`, then `sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' doctor`. `/ready` requires a jobs heartbeat newer than 45 seconds and no maintenance flag. Doctor reports pending/dead-letter totals per Holding without document contents or credentials.
6. Publish only the proxied CNAME `erpx.jesadakorn.com` → the new Tunnel UUID's `cfargotunnel.com` hostname. Tunnel ingress has this hostname → `http://127.0.0.1:3101` and a final 404 rule. Do not modify epsx-dev or n8n.
7. Verify real Resend delivery, HTTPS redirects, auth reset/verification links, secure cookies, all static/PDF assets and registration in an isolated acceptance database before enabling signup in the installed env file and restarting only ERPX web/jobs. Sign up as the agreed admin, verify the email, then run `sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' bootstrap-admin`. This command refuses an absent/unverified account and all other emails.

## Services and secrets

Labels are `com.erpx.postgres`, `com.erpx.web`, `com.erpx.jobs`, `com.erpx.tunnel` and `com.erpx.backup`. Use `sudo launchctl print system/com.erpx.web` to inspect one service; `sudo launchctl kickstart -k system/com.erpx.web` restarts only ERPX web. Jobs recovers outbox leases after process loss and polls committed events once per second. Schedules run every two minutes, leases last 90 seconds with renewal, and an initial attempt plus five retries precedes dead-letter. Inspect failures with doctor; only replay after correcting the cause. Clear `dead_letter_at`, reset `attempts`/`next_attempt_at` for the specific reviewed event via the operator database account; never reset event IDs or projection receipts.

For a planned PostgreSQL restart, enter maintenance and pause web/jobs, then use `sudo launchctl kill SIGINT system/com.erpx.postgres` and wait for the new PostgreSQL PID/readiness before restarting web/jobs. SIGINT is PostgreSQL fast shutdown; SIGTERM is smart shutdown and waits for clients to disconnect. `KeepAlive` starts PostgreSQL again. Do not signal another PostgreSQL cluster.

Daemon environments explicitly set `LC_ALL=C` and `LANG=C`; an unset locale can make PostgreSQL abort with “postmaster became multithreaded during startup” on macOS. Code symlinks (`current` and the Next cache link) explicitly allow traversal with mode 0755; macOS enforces symlink permissions even when the destination directory is accessible. Keep secret/data permissions private when repairing a code link.

`config/production.env` and `tunnel.token` are root:_erpx 0640; `admin.json` is root-only 0600. The superuser credential is used only by installation/backup/restore. Per-database app/jobs manifests are generated under `data/config/`. App roles cannot write read-model tables or migrate schema; each login cannot connect to another Holding database. Limit pools to one per Holding database and a small control pool. The cluster allows 160 connections. Back up all secrets with the data to retain authentication and AI credential decryption.

Node web/jobs/Tunnel logs rotate at 5 MiB with seven archives. PostgreSQL rotates daily across seven weekday files and suppresses SQL/parameters in errors. Startup logs rotate through newsyslog. No HTTP request or document-body logger is enabled. Check Mac disk space; this beta uses a single disk.

## Backup, restore and release rollback

Run a backup now with:

```sh
sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' backup
```

The operator lock excludes concurrent backup/restore. Maintenance denies requests, web and jobs stop, and the command checks that their database sessions have exited before dumping every configured database and copying private files, manifests and secrets. Dumps use PostgreSQL custom format. A snapshot becomes visible only after all files and SHA-256 checksums are written; incomplete `.partial` directories are not restorable backups. Services restart on backup failure. Seven complete snapshots are retained, including a snapshot selected for restore. Backups are local and do not survive disk loss.

Restore with `... erpxctl.py restore '/Library/Application Support/ERPX/backups/<timestamp>'`. It verifies every checksum and the database set, requires the matching installed release, creates a current-state backup, stops services, restores databases and files, restores app secrets, and activates the matching release. Existing cluster login credentials are retained. If restore fails midway, maintenance stays active and web/jobs stay stopped: inspect logs and rerun a verified restore after resolving the failure; remove the maintenance marker only after recovery. For a new cluster, first install the matching release and provision the same database/role names, then restore. Test this on a separate recovery host/cluster before a real disaster.

For a code rollback with unchanged schema, use `... erpxctl.py rollback <release-id>`. It refuses different schema hashes. A schema change requires a matching backup restore; never point an older release at an incompatible schema. Keep the releases referenced by retained backups.

## Upgrade

Build and validate a new isolated release. Create a backup, enter maintenance, and stop only ERPX web/jobs. Copy the release into root-owned `releases/`; do not edit `current` in place. Run its provision tool with the root-only admin credentials to apply append-only checksum-verified migrations and role grants. Keep `_erpx` ownership on data and symlink `.next/cache` to `data/next-cache`. Atomically replace `current`, restart ERPX jobs/web and check readiness and HTTPS. If schema is unchanged, operator rollback is available; otherwise use the pre-upgrade backup. Tunnel and PostgreSQL need no restart for an application release.

## Future domain

Add the new hostname's DNS and Tunnel ingress while the old hostname remains available. Verify the new Resend sender domain. Pause registration, change only `ERPX_PUBLIC_ORIGIN` and `RESEND_FROM` in production.env, restart web/jobs, and test verification/reset/invitation links and secure cookies at the new origin. Existing Holding IDs, database names, object keys and `/holdings/<slug>` paths do not change. Users sign in again on the new cookie origin. Remove old ingress/DNS only after the transition is accepted. Do not add automatic cross-domain session copying.

## Acceptance record

See `.scratch/local-deploy/` for this workstation's build/test logs (not committed, no credentials). Test actual service crash/restart after install without touching other projects. Schedule a full Mac reboot only when the owner is ready; do not reboot automatically. Public registration must remain closed while email verification or HTTPS acceptance is incomplete.
