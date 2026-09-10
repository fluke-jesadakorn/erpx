# Local deployment acceptance — 2026-09-09

Release: `2026-09-08T17-17-08-381Z` (Node 24.20.0, Bun 1.2.17, PostgreSQL 18.6).

## Completed

- Lint, TypeScript checking and the isolated Next.js standalone production build passed.
- Native PostgreSQL integration: 154 tests in 23 files passed, including transaction rollback, concurrent updates, tenant isolation, organization slots, quotas, file revisions/ranges, durable jobs and consistent backup/restore.
- All six HTTP business verification scripts passed. They cover purchasing/sales, identity and invitations, ledger/payroll, four expense browser scenarios, inventory counts and warehouse transfers.
- Production-mode loopback acceptance passed with the canonical public Host/Origin: login, Secure/HttpOnly session cookies, holding and branch routes, scoped API requests, foreign-origin rejection, HTTP-to-HTTPS redirect and foreign-Host rejection. Fixture databases were isolated from the clean production configuration.
- The release contains static and PDF assets. A scan of 2,888 release files found none of the deployment secrets or Tunnel token.
- The relocated Node/PostgreSQL runtime bundle passed an independent PostgreSQL initialization/start/query/stop check. The test cluster uses port 5434; ports 3101 and 5433 were released for installation. Existing development 3100 and PostgreSQL 5432 were preserved.
- Clean provisioning was verified with 20 available slots, no users, no organizations and no Holding bindings. Database login grants were checked separately from migration-owner privileges.
- Resend verified `notify.jesadakorn.com`. The ERPX API key is restricted to sending from that domain and is stored only in the private deployment configuration. A test from `ERPX <no-reply@notify.jesadakorn.com>` to the agreed administrator address reached **Delivered** (message ID `3e1ec31e-75f6-485d-b374-94619e9c383a`).
- Dedicated Tunnel `erpx-local` was created with ID `750f6ab5-1a43-4369-a40a-f6c88ccfb9c6`. Its ingress targets only `erpx.jesadakorn.com` at port 3101, followed by a 404 rule. Existing tunnels were not changed.

## Installed and public

- The system installation completed. All four services run under `_erpx`, with PostgreSQL at 127.0.0.1:5433 and web at 127.0.0.1:3101. macOS-specific fixes explicitly set daemon locale and traversable code-symlink permissions. The installer can resume an inspected initial installation without replacing its cluster or credentials.
- Clean production provisioning had zero users, zero Holdings and 20 available slots across 42 databases.
- Web, jobs and Tunnel recovered after SIGKILL. PostgreSQL recovered after SIGINT (its supported fast shutdown), and web/jobs reconnected. SIGTERM requests PostgreSQL smart shutdown and waits for existing clients; use the documented restart command for an operator restart.
- The proxied DNS record `82def8f2c734f3d3dc60eff597d61b1c` publishes `https://erpx.jesadakorn.com`. Cloudflare reports the dedicated Tunnel healthy with four connections.
- The actual HTTPS endpoint passed certificate validation, HTTP redirect, 12 static assets, PDF worker/WASM and health/readiness checks. Production-mode local checks additionally covered all bundled PDF assets and canonical-origin authorization.
- The first production backup completed at `/Library/Application Support/ERPX/backups/20260908T175253.658170Z`. The daily backup LaunchDaemon is installed for 03:15 Asia/Bangkok, retaining seven complete snapshots. Jobs had zero pending and zero dead-letter events across all 20 slots.
- Public registration is enabled in the installed private configuration. The prepared deployment payload remains closed by default for future clean installations.
- Through the actual HTTPS domain, a disposable owner-controlled email alias completed signup, unverified-login denial, verification, secure-cookie login, password-reset email delivery and reset. The original password was rejected after reset and the replacement worked. Verification email `1a3f91fa-03b7-4f42-bc6c-fc712b463e7a` and reset email `acbfa852-21ca-45b3-9caa-f2856b46dec2` both reached Delivered. This account created no organization or business records. Invitation/provisioning/quota/isolation behavior is covered by the native PostgreSQL suites above.

## Final account setup

The disposable acceptance account has been removed, and its temporary local passwords/session headers/email tokens were deleted. Administrator bootstrap awaits `jesadakorn.kirtnu@gmail.com` signing up and verifying its own email. No first-user administrator grant exists. The machine's non-secret final status is recorded in `/Library/Application Support/ERPX/ops/acceptance-final.json`.

A full Mac reboot awaits the owner's chosen time. Backups remain on this Mac and do not cover disk loss. Billing is disabled.

Detailed local evidence is in ignored `.scratch/local-deploy/` logs. The [operator runbook](README.md) contains installation, recovery and future-domain procedures.
