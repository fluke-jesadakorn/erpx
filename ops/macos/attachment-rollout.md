# Canonical attachment rollout and rollback

This runbook opens the attachment feature once for every registered ERPX workflow. It does not authorize a partial module rollout. Run it independently for staging and production, with that environment's service account, database manifests, private file root and object-store prefix. Never reuse a reconciliation receipt from another environment.

## Preconditions

1. Build and review one immutable release. It must contain Holding migration `0331_attachment_sources_links.sql` and every later attachment migration in the release (currently through `0340_attachment_jobs_storage_grant.sql`), control migrations `0020_billing_attachment_lifecycle.sql` and `0021_billing_attachment_reconciliation.sql`, and `tools/reconcile-attachments.mjs`.
2. Take a verified environment backup before applying schema changes. A code-only staging upgrade is not valid because the attachment release changes the schema hash.
3. Install the exact versions in `clamav/runtime-versions.json`, pin the `clamav`, `libheif` and `poppler` formulae against unattended Homebrew upgrades, and record `brew list --versions` in the private release evidence. Run `install-attachment-runtime.py`, copy all six printed paths into both the web and jobs environment, then run `/bin/sh ops/macos/clamav/verify-runtime.sh` as an operator with those environment values and require success. It verifies the pinned binaries, a mode-0660 `_erpx_scanner` Unix socket, absence of a TCP listener, both launchd registrations and definitions no older than 24 hours.
4. Keep `ERPX_ATTACHMENTS_V2=false`, `ERPX_ATTACHMENTS_READ_ONLY=false` and `ERPX_ATTACHMENT_UPLOAD_FREEZE=false`. Restart both web and jobs so neither process retains old scanner or gate settings.

`/api/health` probes the scanner even while the UI gate is false because the one-release compatibility adapters also quarantine and scan. It reports app and jobs attachment readiness separately and returns 503 if clamd, preview tools, the jobs heartbeat, the freeze setting shared by web/jobs, or the Billing compatibility lifecycle is unavailable. A 200 before the final pass does not mean the global gate may open.

## Schema and online backfill

1. Enter the normal reviewed release maintenance window and apply migrations with the environment's migration manifest and migration roles before starting the new web/jobs release. The migration runner orders names lexicographically, so Holding `0331` precedes every later `033x` attachment migration, and control `0020` precedes `0021`. Verify those receipts in every Holding write database and the control database; do not insert receipts manually.
2. Start the new release with the global gate still false. The scanner is nevertheless mandatory. A temporary degraded health result is expected while historical Billing quarantine drains; it must not be accepted as final readiness.
3. Run the bundled reconciler as the environment service account with its app database manifest and private data root:

   ```sh
   umask 077
   NODE_ENV=production /absolute/installed/runtime/node/bin/node --env-file=/absolute/private/environment.env \
     /absolute/installed/current/tools/reconcile-attachments.mjs --apply \
     > /absolute/private/attachment-backfill-report.json
   ```

   This creates deterministic canonical owners/sets/links and queues historical bytes; it does not assume legacy files are clean. Leave web/jobs running so jobs can drain the scan backlog. Review every `unknown` result. Unknown owner, Company, purpose or object remains quarantined and blocks activation.
4. Repeat the non-final audit without `--apply` until ownership is resolved and the scan backlog is zero. Do not enable the gate to accelerate migration.

## Frozen delta and gate opening

1. Set `ERPX_ATTACHMENT_UPLOAD_FREEZE=true` in the shared private environment and restart **both** web and jobs. Keep the global gate false. Check `/api/health` and require `attachments.uploadFrozen=true` and `attachments.jobs.uploadFrozen=true`; a mismatch means one process still accepts writes.
2. Run the final pass as the same service account, preserving its filesystem ownership:

   ```sh
   umask 077
   NODE_ENV=production /absolute/installed/runtime/node/bin/node --env-file=/absolute/private/environment.env \
     /absolute/installed/current/tools/reconcile-attachments.mjs \
     --apply --scan --checksum --final > /absolute/private/final-attachment-reconciliation.json
   ```

   The command refuses to run unless the gate is disabled, intake is frozen, scanning and checksum verification are requested, clamd/signatures and preview tools are ready, and the Billing and Holding schemas are available. Require `gateReady: true` and a ready final receipt for every Holding plus Billing. Check cross-Company links/intents/storage/publications, missing objects, checksum/size mismatches, quota deltas, duplicate ledger rows, orphan objects, unavailable sources and unknown ownership explicitly; every value must be zero or empty.
3. In one reviewed configuration change, set `ERPX_ATTACHMENTS_V2=true` and `ERPX_ATTACHMENT_UPLOAD_FREEZE=false` for both web and jobs. Restart jobs and web, then wait for a fresh jobs heartbeat. Do not open modules independently.
4. Require HTTP 200 and `attachments.enabled=true`, `attachments.scanner.ready=true`, `attachments.preview.ready=true`, `attachments.jobs.ready=true`, `attachments.migration.ready=true`, and `attachments.billing.ready=true` from local `/api/health` before checking the public hostname. Exercise an authenticated upload, scan, preview/download and bind in the affected pages; health alone is not acceptance.

## Fail closed and rollback

- If clamd is down, signatures are older than 24 hours, FreshClam is not registered, a preview binary is missing, jobs has stale/different freeze state, or reconciliation is not ready, leave the gate closed and repair that prerequisite. Never mark a row clean manually.
- To stop a rollout before activation, keep the gate false. Resolve or explicitly quarantine unknown legacy ownership; do not delete it.
- For emergency application rollback after activation, set `ERPX_ATTACHMENTS_V2=false`, `ERPX_ATTACHMENTS_READ_ONLY=true` and `ERPX_ATTACHMENT_UPLOAD_FREEZE=true` in both web and jobs, then restart both. This blocks reserve, upload, link, bind, withdrawal and publication mutations while preserving authorized reads of clean bound evidence and every source/link/audit row.
- Do not reverse `0331`, `0020`, `0021`, downgrade source state, delete audit history or point an older schema-incompatible binary at the databases. Use the verified pre-upgrade backup only for a full environment recovery decision; Legal Hold and sealed malware remain preserved.
