# ERPX staging on this Mac

Staging is an isolated web/jobs instance under `/Library/Application Support/ERPX-Staging`. It uses `_erpx_staging`, PostgreSQL 18.6 databases prefixed `erpx_stage` on the shared `127.0.0.1:5432` cluster, its own `data/files`, Better Auth secret and service set. Its web server listens on `127.0.0.1:3103`. It does not read production's databases, files, auth secret, sessions or Tunnel token. The old staging cluster at `5435` is disabled and retained for rollback.

## Install from a pinned release

The following installer is for **legacy isolated-cluster installs only**. It initializes PostgreSQL on `5435` and must be updated before provisioning another instance on the shared cluster. The historical command was:

```sh
sudo /usr/bin/python3 ops/macos/install-staging.py \
  --release .runtime/releases/latest \
  --payload .runtime/deployment \
  --test-env .env.local
```

The installer refuses occupied ports and unexpected existing staging data. It can resume its empty directory skeleton or the recognized PostgreSQL initialization/provisioning state, including databases whose migrations started before an earlier attempt stopped and completed manifests before admin bootstrap. It preserves the staging secret, PostgreSQL administrator credential and generated database-role credentials. A rebuilt release can be activated during this resume without recreating the staging cluster. It creates the `_erpx_staging` account and separate LaunchDaemons for PostgreSQL, jobs and web. It writes `erpx_stage_*` roles/database manifests. A fresh administrator password is generated for this staging account and written once to `.scratch/erpx-staging/admin-credentials.txt` with mode `0600`, owned by the invoking user. The password is not printed to the terminal.

The installer imports only Stripe test variables. If a subscription or POS test secret is present, it must start with `sk_test_`; corresponding test webhook secrets must start with `whsec_`. Production payment keys are never copied. `POS_STRIPE_LIVE_ENABLED` and `PMS_STRIPE_LIVE_ENABLED` are false, and `PMS_STRIPE_BOOKING_ENABLED` stays false unless a test subscription key is configured. Email delivery uses the database sandbox; Resend credentials are not copied. Public signup remains disabled after the staging administrator is created.

The initial staging database has fresh schema and synthetic Acme/Northstar workspace slots. It contains no production customer records. Add any further demo records through staging workflows or explicitly synthetic fixtures.

## Services and acceptance

The schema-changing AI Gateway and target Billing release must follow the
[reviewed migration, backup and rollback gate](billing-target-staging-migration.md).
The code-only upgrade path below does not apply to this release.

The active service labels are `com.erpx.shared-postgres`, `com.erpx.staging.jobs`, `com.erpx.staging.web`, and `com.erpx.staging.backup`. To inspect staging, use:

```sh
sudo launchctl print system/com.erpx.shared-postgres
sudo launchctl print system/com.erpx.staging.jobs
sudo launchctl print system/com.erpx.staging.web
curl -fsS http://127.0.0.1:3103/api/health
curl -fsS http://127.0.0.1:3103/api/ready
```

PostgreSQL listens only on loopback. Check that staging manifests contain only `erpx_stage_*` databases on `5432` and that its file directory is independent before publishing the hostname. Leave production web/jobs on `3101` untouched.

## Public hostnames

The target `erpx-local` Tunnel mapping is configured in Cloudflare Dashboard:

| Hostname | Local origin | Environment |
| --- | --- | --- |
| `dev-erpx.jesadakorn.com` | `http://127.0.0.1:3100` | Development homepage |
| `console-dev-erpx.jesadakorn.com` | `http://127.0.0.1:3100` | Development console |
| `erpx.jesadakorn.com` | `http://127.0.0.1:3103` | Staging homepage |
| `console-erpx.jesadakorn.com` | `http://127.0.0.1:3103` | Staging console |

All four DNS records must be proxied and point to the same `cfargotunnel.com` target. These first-level console names are covered by the zone's Universal SSL certificate. In the zone's two Windows/Android redirect rules, exclude all four ERPX names. Check `/api/health`, TLS, the public homepage, and console sign-in on both environments. Sign in separately; never copy sessions between them.

Production remains private on loopback `3101` and shares PostgreSQL `5432`. If staging fails after publication, restore the previous staging release and configuration from its backup. Keep both staging Tunnel routes on `http://127.0.0.1:3103`; never route staging traffic to the development service on `3100`.

## Bootstrap implementation notes

`bootstrap-admin-service.ts` contains the shared implementation without a CLI entry point. The staging bundle imports this module so that bundling cannot execute the standalone admin CLI before creating the staging user. Credential accounts use Better Auth's `local:credential` issuer; environment isolation comes from the separate database and auth secret.

On 2026-09-24, the local staging install passed health/readiness, a normal password sign-in, an authenticated workspace request, and the disabled-signup check. PostgreSQL runtime logins were verified as non-superusers. No production records were imported. Later that day, the same staging databases moved to the shared `5432` cluster; see [postgres-consolidation.md](postgres-consolidation.md).

Publication status (2026-09-24): Cloudflare `erpx-local` routes `erpx.jesadakorn.com` to staging on `3103` and `dev-erpx.jesadakorn.com` to development on `3100`. Both DNS CNAME records are proxied. Windows and Android redirect rules exclude both names. Both public health/readiness endpoints, login pages, and staging password sign-in were verified over HTTPS.

## Homepage and console cutover

The code-only staging activation command is `sudo /usr/bin/python3 ops/macos/upgrade-front-staging.py --release <reviewed-release-path>`. Run the same script with `--check` first. For the 2026-09-25 homepage cutover, build from the isolated `.scratch/stage-marketing-src` source, which matches the installed staging schema. The active release with the contrast and system-theme update is `.scratch/stage-marketing-src/.runtime/releases/2026-09-25T11-06-09-644Z`; its code-only check and local host matrix passed. The main workspace's release includes unrelated pending migrations and fails the code-only check. The activation script backs up staging, updates only staging's application and marketing origins, activates the reviewed release, and rolls back the code and environment on failed local verification. It does not restart private production on `3101`.

The old public page links redirect to the environment's console with path and query preserved. The two Stripe test webhook endpoint URLs now point to `https://console-erpx.jesadakorn.com/api/pos/stripe/webhook` and `https://console-erpx.jesadakorn.com/api/v1/stripe/webhook` with their original signing secrets. On 2026-09-25, Stripe delivered test-mode `payment_intent.canceled` to the Connect endpoint and `payment_intent.created` and `payment_intent.canceled` to the billing endpoint; all received HTTP 200 before and after the final staging activation. These were unconfirmed, then canceled PaymentIntents with no payment method or charge. The Connect test account is not bound to a staging company, so its signed event was correctly acknowledged as ignored. The `acme` Connect OAuth redirect URI on the console was added and set as the default in Stripe Dashboard; the old URI is retained for rollback. The marketing host now returns 404 for both webhook paths. A full OAuth account-linking flow still needs an authenticated ERPX test user.

After activation, staging administrator sign-in through the local `3103` service using the console Host and HTTPS origin returned a secure, host-only cookie and an authenticated session. That cookie did not authenticate on the development console at `3100`. Both public homepages and console sign-in pages were opened in Chrome, and each homepage's Open console action reached the matching console. An interactive public sign-in and a full Connect account-linking flow remain unverified.
