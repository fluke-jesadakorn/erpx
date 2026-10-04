# PMS Front and POS Front rollout

Both Front applications run in the existing Next.js web process. Host and API
allowlists in `src/proxy.ts` keep their pages, authentication cookies and daily
commands apart. Front sessions use separate Better Auth cookie prefixes and
host-only cookies. A remembered location/register is only a device preference;
the server checks the employee's current assignment on every request.

| Environment | PMS origin | POS origin | Loopback service |
| --- | --- | --- | --- |
| Development | `https://dev-pms.jesadakorn.com` | `https://dev-pos.jesadakorn.com` | `127.0.0.1:3100` |
| Staging | `https://stage-pms.jesadakorn.com` | `https://stage-pos.jesadakorn.com` | `127.0.0.1:3103` |
| Production, after acceptance | `https://pms.jesadakorn.com` | `https://pos.jesadakorn.com` | private `127.0.0.1:3101` |

## Verification and activation

Development verification uses the authorized development hosts above. Current employee assignment and app origin are rechecked on every request; Front cookies do not grant Console or other-Front access. Schema and code rollout follow the matching immutable release and owner contracts. Staging activation, health checks and route changes require an explicit staging request and use [the staging runbook](staging.md#front-application-activation-explicit-staging-request-only). Private production publication requires separate acceptance.
