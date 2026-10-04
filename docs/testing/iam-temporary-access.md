# Temporary emergency access

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

Account Security now closes the self-service emergency-access lifecycle. A designated recovery administrator must complete fresh MFA before activating a temporary elevation, which is scoped to one Platform or Holding company and expires no later than one hour or the authorizing assignment's expiry. The page lists only elevations attached to the signed-in user and current browser session; each entry shows its scope, reason and expiry. The same session can revoke its elevation early, and the authorization lookup rejects that session immediately after the revocation commits.

Activation and revocation write immutable IAM audit evidence and an idempotency receipt in the same PostgreSQL transaction as the session change. Reusing a key with the same request returns the committed result, including when duplicate requests arrive concurrently. Reusing that key for a different operation or payload fails with a conflict. Revocation cannot target another user or another browser session. Account Security reports Holding scopes whose status could not be checked instead of treating an incomplete check as an empty list.

## Automated scenario

`tests/iam.test.ts` creates an isolated recovery administrator, requires a fresh MFA session, and exercises concurrent activation retries, conflicting payloads, current-session listing, expiry filtering, concurrent revocation retries, immutable audit entries and immediate denial from `loadIamAuthority`. The test also leaves a different browser session's elevation intact. Captured automated output is recorded in [2026-09-23-iam-temporary-access.txt](evidence/2026-09-23-iam-temporary-access.txt).

## Manual role walkthrough

1. Configure a separate recovery account with `login_policy='emergency'`, an active `holding_owner` assignment for the selected Holding (or `platform_admin` for Platform), and an enrolled authenticator.
2. Sign in as that recovery account, open **My access → Temporary emergency access**, verify an authenticator code, then activate access with the target company and a reason.
3. Reload Account Security. Confirm the same scope, reason and expiry remain visible for this browser session.
4. Deactivate the entry, then retry a request that required the emergency elevation. It must be denied. A second browser session's independent elevation must remain active.

The automated gate covers the owner service and PostgreSQL transaction behavior. Browser acceptance, independent company-owner acceptance, production recovery operation, off-machine backup durability and signed recovery-time objectives remain separate and pending.
