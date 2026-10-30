# Development Holding addresses

When the development Holding routing profile is enabled, `acme.jesadakorn.com`, `northstar.jesadakorn.com`, and `erpx-internal.jesadakorn.com` map to their exact registered Holdings. Account remains at `account-dev-erpx.jesadakorn.com`. These registrations do not grant Company or record access; the original owners recheck authority and organization lineage. Unknown hosts and cross-Holding references are rejected. No production or staging registration is implied.

Public workspace paths identify a branch or department and app, for example `/bangkok/finance` or `/bangkok/finance-dept/finance`. A Company-wide entry must explicitly select a Company. `/__erpx/<app>/api/` and `/__erpx/<app>/_next/` separate transport for independent applications. Work remains a distinct application in this delivery.

Account SSO issues host-only child credentials bound to the exact Holding and development environment. Device selection uses opaque handles and a revision; browser responses do not expose session tokens. Account switching and session revocation recheck the current owner state. Migration 1073 replaces conflicting branch and department route slugs while preserving node identity and restoring the immutable-route trigger.

These are development contracts. The source change does not itself publish DNS, change Tunnel routes, deploy production, or establish acceptance.
