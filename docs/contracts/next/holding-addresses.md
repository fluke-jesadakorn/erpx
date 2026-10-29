# Holding addresses

One exact registered development hostname identifies a Holding, not an application or a Company. Company remains a legal entity and data boundary inside the Holding. The app registry and the exact hostname registry are independent. Wildcard DNS never confers trust. Production `{holding}.erpx.com` is reserved as a future format only.

| Context | Development address |
| --- | --- |
| Explicit selection | `https://acme.jesadakorn.com/` |
| Branch | `/bangkok/finance` |
| Branch department | `/bangkok/finance-dept/finance` |
| Direct Company department | `/departments/accounting/finance` |
| Company | `/finance?company=acme-co` |
| Public | `/book`, `/customer-portal`, `/legal-portal`, `/forms/<formId>` |

Root and unscoped app entry open the scope selector; browser memory cannot select a workspace automatically. Company is resolved from current active branch/department lineage and checked with current owner authority. Company selectors accept a current Company slug or ID. Cross-Holding paths, tokens or API references cannot override the host. A nonexistent or mismatched structure returns404. ERPX provider management is restricted to `erpx-internal.jesadakorn.com` and retains existing Platform authority.

The gateway selects an independent Next app from the public path. Shared constructors translate owner URLs into public Holding addresses while retaining query values, fragments, Company and record identity. Public URLs translate to existing owner pages internally. API payloads, original transactions, public token validation, invitations, purchase gates and authorization remain with their owners. All apps use `/__erpx/<app>/api/` and `/__erpx/<app>/_next/` for transport; the gateway forwards API requests to the original handler path. Cross-app navigation loads a new document and retains the existing unsaved-work confirmation.

Account remains the central issuer on `account-dev-erpx.jesadakorn.com`. Its existing exchange issues an expiring, revocable child bound to the exact registered Holding host and environment with a host-only cookie shared by apps in that Holding. Requests still recheck current Company and record authority. Account selection/continuation retains the target app. A child on Acme cannot authorize Northstar.

Conflicting branch/department slugs use `-branch`/`-dept`, followed by a numeric suffix if needed. Migration1073 locks organization nodes, records replacements, preserves IDs and lineage, updates hierarchy revision, then restores immutable route triggers in the same transaction. New organization routes reject reserved app/system names. There are no old-slug aliases.

Former development product/shared hosts return generic404, without redirects or business forwarding. Account remains active; Console retains only exact health/ready reads. The local gateway remains3100; private production3101 and PostgreSQL5432 remain available. Staging is outside the authorized work. See [ADR-051](../../adr/ADR-051-holding-addresses-and-workspace-paths.md) and the [development runbook](../../../ops/macos/holding-development.md).
