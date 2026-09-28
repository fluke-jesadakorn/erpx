# Billing target staging migration gate

Status: **pending reviewed backup and restore rehearsal**. This is a staging-only runbook for the Sandbox AI Gateway and rated Holding billing release. The code-only `upgrade-front-staging.py` path is inapplicable because this release adds PostgreSQL schema. Do not route staging to the development process or touch the private production service.

## Scope and preflight

- Confirm the staging web/jobs processes use only `erpx_stage_*` databases on shared PostgreSQL `5432`, the staging files directory, Stripe `sk_test_` and `whsec_`, and `127.0.0.1:3103`. Record the running release identifier and migration inventory for every staging Control and Holding write database.
- Review *all* pending migrations between installed staging schema and this source, including Control `0023`–`0029` and Holding write `0342`–`0346`. Control `0029` adds the point-in-time active-seat count to the usage snapshot; it is informational and cannot replace the historical seat intervals needed to rate a month. The workspace has unrelated pending migrations; a reviewer must approve the full ordered delta, not only these billing migrations. Check DDL locks, disk headroom and backup capacity, and run the exact migration sequence against an isolated copy first.
- Capture a consistent custom-format `pg_dump` for every affected `erpx_stage_*` database, a staging files/config snapshot, and the active release. Store checksums and restricted permissions outside the release directory. Test restore into **new isolated staging-named databases**, verify row counts and migration versions, then mark backup/restore rehearsal passed. Never overwrite private production databases.

## Activation

1. Stop staging jobs and web after the verified backup, leaving private production and shared PostgreSQL running. Apply only the reviewed ordered migrations through the staging migration credentials; do not run `bun run db:migrate:local` against staging.
2. Deploy a release built from the same reviewed source and migration set. Keep `billing_target_sandbox_holdings` empty unless an explicitly designated synthetic test Holding has passed the target pricing gate. Confirm new target invoices cannot issue from incomplete AI/storage/cost data, and existing ADR-028 invoices remain unchanged.
3. Start staging jobs and web on `3103`. Verify local and public health/readiness, Platform Admin/Auditor and Holding/Branch Usage pages, Stripe test-key rejection of Live objects, a synthetic manual and consented card invoice, webhook replay, Finance test-ledger reconciliation, and no cross-Holding visibility. Record test invoice IDs and source/release checksums.

## Rollback

If migration, startup or reconciliation fails, stop staging jobs/web and restore the **matching release, config, files, and all affected databases** from the verified backup into staging-only databases. Verify restored migration versions, row counts, `3103` health and public staging behavior before reopening staging. Do not replay a pre-migration backup over Stripe state silently: reconcile any Sandbox invoices created after the backup by Stripe invoice ID, void unpaid test invoices where appropriate, and retain an audit record of the recovery. The private production `3101` service and its databases remain untouched.

The release gate stays pending until backup checksums, restore rehearsal, full migration inventory, reviewer, rollback evidence, and browser acceptance are recorded. A passing local test or code-only check does not complete this gate.
