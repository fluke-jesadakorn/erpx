# Pilot hostname readiness — 2026-09-23

Read-only check from the ERPX development Mac:

| Hostname | `/api/health` result | Interpretation |
| --- | --- | --- |
| `dev-erpx.jesadakorn.com` | DNS lookup failed (`curl` code 6) | The planned development hostname is not configured or resolvable from this host. |
| `erpx.jesadakorn.com` | HTTP 200 | The existing public development route remains healthy. |

No DNS, Cloudflare Tunnel rule, production listener, or public origin was changed. The plan's hostname sequence cannot advance until `dev-erpx.jesadakorn.com` is configured and its authenticated development journey is verified. The production hostname must continue pointing at development under the later workspace instruction until a separate launch decision.
