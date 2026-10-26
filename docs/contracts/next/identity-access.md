# Identity and access — implementation and operations

Apps is the staff product owning Feed and the application selector. The retired Work host/audience is no longer accepted; its child sessions are not converted to Apps sessions. Normal Account SSO and other product sessions retain their existing policies. Apps workspace memory is scoped by account and reconciled against current owner-authorized Company/Workspace choices before opening Feed.

The prospective [product SSO contract](product-separation.md#sso-protocol) adds creation-time audience/parent binding behind disabled development flags. Legacy login/session behavior remains in effect until product cutover acceptance. The new protocol is not yet accepted for account switching, directory callback migration, Holding tombstones, queued authority or Push.

The approved redesign adds local accounts, optional simultaneous directories, immutable custom roles, scoped grants, an access matrix and reviewed selective migration. [ADR-025](../../adr/ADR-025-multiple-identity-providers-and-scoped-replacement.md) records the identity boundary and supersedes the obsolete single-directory assumptions.

## Application surfaces

Customer IAM starts at `/holdings/{holdingSlug}/iam`; existing Branch/Department routing narrows the same Company services. Sections are `overview`, `users`, `roles`, `groups`, `matrix`, `connections`, `reviews`, and `my-access`. Platform IAM starts at `/platform/iam`. `/my-access` shows the signed-in person's assignments across accessible workspaces even without a selected role; `/security` provides MFA and session controls.

The matrix compares up to twelve roles or user assignments, supports action/module, provider, assignment status and scope filters, and explains Allowed, Denied, Conditional and Unavailable decisions. User columns evaluate individual assignments without combining roles. Details include source, scope, expiry, policy revision and remaining conditions. Export requires its own permission, retains the selected work-context headers, escapes spreadsheet formulas, and records an audit event. Arrow keys navigate cells; the explanation receives focus for keyboard/mobile use.

The read-only inspector accepts person/user, assignment, workspace, action and optional typed record. It invokes record read policies without selecting a session or authorizing commands. An action outside that record type is Unavailable. Record ownership is checked independently of an administrator's broader read rights. Write inspection remains Conditional where command inputs, workflow state or amounts are needed.

Reviews & audit searches the complete scoped IAM history by action, actor ID or target ID. Results use a stable timestamp/ID cursor in pages of 100 events; loading older events retains the search and Company boundary.

Open access-certification campaigns notify every currently eligible company-level `iam.review` holder seven days before the due time and again after the campaign becomes overdue. The scheduled worker rechecks the assignment chain, role revision and principal state on every run, emits each campaign/phase/user alert once, and stops alerting completed or cancelled campaigns. Inbox rechecks the same current authority before exposing a notification and links directly to the campaign in Reviews & audit.

## Policy and storage

- `src/lib/identity_access/iam.ts` owns shared contracts; `src/lib/identity_access/permissions.ts` owns live and preview capabilities and built-in compatibility definitions. Preview modules cannot supply executable grants.
- `iam-store`, `iam-transactions`, `iam-service` and `iam-inspector` resolve policy, enforce parent bounds/revisions and implement review workflows. Existing People access and invitation APIs adapt to these services.
- Control migrations `0006_iam`, `0007_iam_catalog` and Holding-write migrations `0020_iam`, `0021_iam_catalog` are additive. The catalog update publishes built-in revision 2 for generic live-record permissions and invalidates old contexts; it preserves assignment validity/history.
- Permissions are explicit and default-deny. Scope, staff eligibility, ownership, workflow participation, approval limits and independent review remain mandatory. Customer role IDs and permission namespaces cannot grant Platform access.
- IAM writes require `Idempotency-Key`, expected resource/role revisions and selected-context evidence. Review approval locks and rechecks the original author's authority, reviewer ceiling, Person independence, current policy and fresh MFA. Sensitive reviews cannot authorize their own beneficiary.
- Queued human-authorized work captures principal, identity, policy and assignment revisions. Workers reject unversioned legacy jobs and recheck authority before provider work and publication. An explicit retry captures fresh authority.
- Revoking one's session records revocation in the affected Company stores before deleting the global session, so previously queued authority is invalidated too.
- Work-summary reads reuse identical eligibility queries only within that summary request. The cache is bounded and does not cross requests or command transactions; commands still recheck current authority.

## Directory provisioning

Connections are opt-in: **Choose provider → Configure → Test → Preview users/groups → Define mappings → reviewed activation**. Saving never imports, grants, changes local login or enables provider sign-in. Multiple connections may be active in a Company. Only a company-scoped directory administrator edits a connection; delegated mapping administrators can narrow mapping scope within their parent grant.

Set `ERPX_IAM_SECRET_PROFILES` to an absolute, private JSON file on the web and job hosts. Provision the file outside Git and releases, readable only by the runtime account. PostgreSQL stores the profile name and non-secret configuration. A profile must explicitly approve each connection ID and hostname:

```json
{
  "corporate-directory": {
    "connectionIds": ["UUID_RETURNED_AFTER_SAVING_THE_CONNECTION"],
    "allowedHosts": ["directory.example.com", "sso.example.com"],
    "bindDn": "uid=erpx-reader,cn=users,dc=example,dc=com",
    "bindPassword": "PROVISION_EXTERNALLY",
    "clientSecret": "PROVISION_EXTERNALLY_IF_OIDC",
    "caPem": "PEM_CA_CHAIN_IF_PRIVATE_CA"
  }
}
```

Save the inactive connection first to obtain its ID, authorize that ID in the profile, and then test. Secrets/passwords never belong in the connection form, logs, audit evidence or jobs. `allowedHosts` includes approved OIDC discovery/JWKS/token endpoints as well as the LDAP host; redirect and continuation URLs are validated. LDAPS always validates the certificate and hostname. There is no insecure-TLS switch.

| Provider | Configuration | Immutable identity |
| --- | --- | --- |
| Windows AD | `mode=ldaps`, authority, LDAPS URL, user base DN, optional group base DN/user filter | authority + `objectGUID` |
| Entra ID | `mode=oidc`, tenant ID, tenant-specific issuer, client ID | tenant + object ID; issuer/subject retained |
| Univention UCS LDAP | `mode=ldaps`, authority, UCS LDAP URL/base DN | authority + `entryUUID` |
| Univention UCS Keycloak | UCS LDAP settings plus `mode=oidc`, issuer and client ID | verified OIDC `entryUUID` mapping to UCS LDAP; issuer/subject retained |

Entra registers callback `/api/auth/callback/directory-{connectionId}` at the ERPX public origin. Use a tenant-specific issuer (`https://login.microsoftonline.com/{tenantId}/v2.0`), never `common`. Grant the application the directory read permissions required by the configured Graph queries and tenant consent, including user enumeration/status and group/transitive memberships. Graph synchronization supplies memberships even when an ID token omits groups or uses group-claim overage. See Microsoft's [transitive membership API](https://learn.microsoft.com/en-us/graph/api/user-list-transitivememberof?view=graph-rest-1.0).

UCS compatibility targets **UCS 5.2 and Keycloak app 26.x**. Configure a read-only immutable LDAP `entryUUID` mapper into the signed ID token. For connection testing, provision a current `verificationIdToken` in the external profile for a consenting test identity; ERPX verifies its signature, issuer, audience and LDAP identity correlation. This is deployment-specific mapper configuration, not an assumption that UCS provides that claim automatically. Use the documented [UCS LDAP interface](https://docs.software-univention.de/ucs-operation/5.2/en/domain-infrastructure/ldap-directory.html) and [Keycloak application configuration](https://docs.software-univention.de/keycloak-app/26.x/configuration.html).

OIDC uses Better Auth's verified discovery/JWKS callback, issuer/audience/signature validation, state, nonce and PKCE. Implicit signup and account linking are disabled. Directory principals have synthetic internal authentication addresses; their directory contact emails remain separate, so duplicate contact emails do not link accounts.

## Synchronization and mappings

The existing PostgreSQL job runner reconciles active connections every five minutes. LDAP synchronization is paged, includes nested group closure and AD ranged membership, and has bounded depth/page/object limits. Exceeding those bounds, referrals, malformed pagination, TLS failure or a partial read fail the snapshot. Older concurrent snapshots cannot overwrite newer evidence. Login refreshes directory evidence; stale evidence requires a successful refresh before dependent access.

A complete snapshot updates confirmed disabled/deleted users and removed memberships. A failed refresh records a bounded error category and makes dependent evidence stale; it never restores local login. Disabling a connection invalidates dependent authorization while retaining its migration policy.

Group mappings are independently reviewed and carry role revision, parent assignment, scope and expiry. Manual grants and group-derived grants remain distinct. Only already approved directory members can receive mapped grants: a later group addition never migrates a local employee. Group removal revokes only derived grants. A revoked mapping grant is not silently resurrected on rejoin; propose a reviewed new mapping or grant/renewal when reauthorization is intended.

## Selective migration and recovery

1. Provide at least two independently authorized customer administrators with enrolled authenticators. Initial customer commissioning is an operator/customer-authorized provisioning prerequisite; there is no cross-namespace Platform shortcut or self-approved second-administrator grant.
2. Independently designate a local owner as an emergency administrator. The account needs a local credential and MFA. The final active recovery owner and its parent authority cannot be revoked. Recovery use requires fresh MFA, a reason and a bounded session (maximum one hour).
3. Test the active provider and preview the directory. Select explicit identities or a group, resolve it to a fixed list, and match each to a reviewed Person/local principal. Email is only a search attribute.
4. Specify the replacement role grants and validity. Existing roles do not transfer automatically. Prepare the batch; local employees continue working while review is pending.
5. A different authorized Person verifies MFA within five minutes and approves. Activation rechecks provider evidence, identity linkage, role revisions, grant ceilings and emergency readiness inside the transaction.
6. ERPX creates or reuses an idempotent distinct directory principal, inserts the reviewed grants, marks the local principal superseded in that Company, invalidates its Company contexts/grants and updates current staff linkage. Historical audit/record actor IDs remain unchanged; Person-aware participation and independence checks preserve continuity.

The shared local authentication account may still sign in for another Company. It cannot regain access to the migrated Company through old sessions, a password reset, invitation activation, or a direct API. Group removal and disabling the connection never undo migration. Failed activation rolls back customer changes; an idempotently created control identity without customer grants has no access and is safe to retry. Expired or obsolete approvals need a new proposal.

Recovery and access renewal must be arranged before parent authority expires. Assignments default to 90 days capped by authorizing validity, and renewals create new assignments. There is no automatic expiry extension, directory fallback, or authentication/MFA bypass.

## Account recovery

Password recovery accepts an email address without revealing whether an account exists. For an eligible local account, ERPX creates a one-hour Better Auth verification and an account-recovery delivery job in the control database. A newer request deletes the older verification and cancels older pending delivery work, so only the latest link can be used.

The delivery job stores the reset URL encrypted with an account-recovery key derived from `BETTER_AUTH_SECRET`; plaintext tokens are not retained in the delivery table, events or logs. The directory worker claims jobs with a lease, uses the stable job ID as the provider idempotency key, records the provider message ID, recovers interrupted claims and retries with exponential backoff for at most six attempts. Expired jobs and deleted accounts are cancelled before a provider call.

A successful password reset deletes every active session for the account and records immutable completion evidence. Session deletion has a fail-safe path if evidence insertion fails. Resetting a shared local credential never restores access removed by a Company directory migration or revoked assignment. Invalid, expired and superseded links show a clear replacement-link action without displaying a password form.

## Verification and deployment

> Recorded observations apply to their stated date/environment. Current coverage and remaining acceptance are in the [verification inventory](../../status/capability-delivery-matrix.md); old gaps do not override current owner behavior.

Run typecheck, lint, the integration suite, IAM/auth/workflow Playwright suites, and `bun run deploy:build`. The release build is isolated from the running development output. Apply additive migrations with the established local provisioning command; never reseed user data as part of IAM migration. Reviewers must reselect work roles after a revision invalidates a context.

The application and PostgreSQL adapter tests rehearse local-only operation, inactive connectors, immutable collisions, independent review/MFA, parent expiry/revocation, complete versus partial synchronization, explicit migration, preserved history and group changes. Provider tests use controlled LDAP/Graph responses and signed OIDC tokens; these are not live tenant certification.

**Live provider acceptance remains environment-dependent:** install approved profiles, certificate trust, reachable LDAP/Keycloak endpoints and Entra tenant consent, then verify all four sign-in paths, refresh/outage behavior and an explicit pilot batch with independent operators. No live provider profile or tenant was supplied for this implementation.

Development must remain on port 3100, with local and public `/api/health` returning 200 and public IAM browser pages verified. The production service on 3101 and Tunnel routing remain intact. Test results for this change are recorded in [IAM acceptance](../../testing/identity-access.md).

## Person field commands without IAM read

A current HR/person-field writer may preview and apply permitted custom values through `people.custom_attributes.write` without acquiring `iam.read`. The command uses only the selected, scope-checked Person, Company field definitions and that Person's current values. Field ownership, read/write roles, Branch override rules, optimistic versions, preview freshness, command receipts and existing reconciliation remain enforced. Its preview does not disclose groups, policies, access candidates or unrelated people. Full management reads continue to require `iam.read`; this does not restore IAM authority removed by the HR/IT separation.

## Development device accounts and shared role menu

Apps, Admin and the fifteen staff product hosts expose the same account menu. Apps keeps account access in its header; Admin and staff product shells also expose it in their Sidebar. Adding an account uses the standard right form drawer. Account owns credential verification, its existing Directory callbacks and the saved device sessions; Company, Platform and Provider selectors retain their original owner eligibility, assignment versions and optimistic revisions. This implements the bounded development switcher and does not certify whole-product production acceptance.

The Account-only `/api/v1/account/device-accounts` family returns display summaries, opaque account handles and a browser selection revision. Its `initialize`, `sign-in` and `active` POSTs and root DELETE require an exact owned staff/Account Origin; credentialed CORS never trusts a wildcard, public host, staging or future origin. Session tokens and passwords never enter the cross-origin JSON response. Control migration `0040_device_accounts.sql` groups only sessions proven by signed Better Auth cookies, retains up to five live accounts and serializes sign-in reservations. A sixth new account fails before authentication; reauthentication of a saved account remains available. Existing Account sign-in and Directory callbacks join the same selection. Cookies stay secure, HttpOnly and host-only, and original session/MFA deadlines are unchanged.

A selection holds the original Control session-first fence, invalidates old Holding Work contexts through the owning IAM revocation fences and removes child sessions before acknowledging success. Saved Account parents remain available for explicit reselection; independent devices retain their own parents, contexts and Push subscriptions. Queued Work authority rejects the old Holding tombstones and Push rejects the deleted child. A partial Holding failure does not acknowledge a new selection: already invalidated contexts remain closed and the browser must re-enter SSO. This is a fail-closed authentication continuation, not a distributed business transaction. Pre-migration product cookies without a browser binding re-enter normal SSO; a normal exchange adopts the signed Account parent.

Each staff host owns GET `/api/v1/account/switcher-context`, POST `/api/v1/account/role-selection` and GET `/account/continue`. Focus, restored pages and submission recheck the local product identity. Changed accounts hide and suspend the mounted old draft. The user can explicitly discard it and continue, or restore the original account through a same-origin, verified SSO continuation while retaining the mounted draft; native role/version checks still govern resubmission. No old command is replayed automatically. If the new account cannot reopen the source workspace, the continuation returns to Apps. The closed Account `/account/device-return` bridge preserves an existing normal Directory login return without accepting arbitrary destinations.

Suspension and dirty tracking include portaled drawers. Repeated focus checks cannot close a recovery sign-in drawer or reveal a paused draft after a network failure. An uncertain mutation blocks another attempt until the Account selection has been read successfully. The role menu remembers its Company/workspace hint per account in tab storage and restores it only after current owner-filtered discovery. The Apps launcher separately remembers its selection and recent app identifiers in account-separated local storage, as specified below. The Directory option accepts a Company-provided Account sign-in link, extracts its known connection and uses the existing Directory verification page with a sanitized return; it forwards no arbitrary callback query. A rejected normal callback, including an extra sixth account, removes its new unselected parent and retains the previous selection. Live Directory-provider acceptance still requires an approved reachable tenant.

### Apps company-first selection

The Apps root uses three vertical steps on one page: Company → Workspace → App. Company cards include their Holding label and search. Workspace choices use the exact Company and scope from current authorized product and Company administration launch rows. Departments sit inside expandable Branch groups; a Department-only assignment never creates a selectable Company or Branch workspace. Company administration-only entries remain selectable even when that workspace has no employee apps. This presentation is confined to `packages/workplace/src/entries/workspace.tsx` on the Apps host; Admin and other product entries retain their existing behavior.

Completed steps collapse into summaries with Change buttons. Editing preserves the current selection until a replacement is chosen. A different Company clears the old workspace and app search, then auto-selects only a sole valid workspace. A different workspace recalculates its exact app links. Later steps stay pending until their prerequisites are selected. Explicit forward/backward changes move focus to the active heading and scroll it into view, respecting reduced motion. Sole companies/workspaces are selected automatically; opening an app always requires user activation.

The App step deduplicates by product identifier within the selected workspace. It initially shows at most six apps, ordering available recent apps from this Company first and filling with the existing system order. Search includes apps outside the initial six; View all apps expands and collapses the same step. Existing history/settlement/authorized-work labels and native SSO links remain intact. Company administration appears only for the selected exact scope. Provider administration remains a separate secondary entry. Migration recovery entries, when available under the current retirement policy, are in a collapsed section. Apps has no duplicate product Sidebar.

Browser memory is optional, versioned and keyed by the authenticated account. It stores Company/workspace identifiers and recent product identifiers per Company, never SSO URLs, permissions or grants. Current server discovery validates remembered selections and filters history before any link is rendered. Unavailable remembered Company/workspace choices return to the required step with a notice. Invalid JSON, denied storage and full storage preserve ordinary selection and SSO. Step selection, search and expansion require no new API, server mutation or route transition; original owners reauthorize the activated launch.
