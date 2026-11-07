# PMS Front and POS Front rollout

## Current development

PMS and POS are independent owned applications behind gateway3100. Staff/public browser addresses use the exact registered Holding host and app paths, for example `https://acme.jesadakorn.com/pms?company=acme-co` and `https://acme.jesadakorn.com/pos?company=acme-co`. Account remains central. Former `dev-pms.jesadakorn.com`, `dev-pos.jesadakorn.com` and product development hosts return generic404; they are not aliases. See the [Holding address contract](../../docs/contracts/next/holding-addresses.md) and [development runbook](holding-development.md).

App-owned Proxy/route composition and original owner policies separate staff, public and daily-command access. Child sessions bind to the exact Holding/environment; Company/location/register selection is checked against current authority. Device preferences do not grant access. Native Front navigation remains specialized, and normal Account SSO is required. Gateway3100 owns app processes within3200–3299; do not start a second shared web process.

## Installed topology reference

The following pre-Holding installed topology is retained deployment metadata, not authorization to publish or inspect it:

| Environment | PMS origin | POS origin | Loopback service |
| --- | --- | --- | --- |
| Staging, explicit request only | `https://stage-pms.jesadakorn.com` | `https://stage-pos.jesadakorn.com` | `127.0.0.1:3103` |
| Production, after separate acceptance | `https://pms.jesadakorn.com` | `https://pos.jesadakorn.com` | private `127.0.0.1:3101` |

## Verification and activation

Development verification uses current Holding paths and normal authentication. Verify actual Front commands, drafts, receipts and provider callback authority on the relevant candidate; health200 or an old host receipt is insufficient. Schema/code rollout follows the matching immutable release and owner contracts. Staging activation, health checks and route changes require an explicit staging request and its [runbook](staging.md#front-application-activation-explicit-staging-request-only). Private production publication and live payments require separate acceptance.
