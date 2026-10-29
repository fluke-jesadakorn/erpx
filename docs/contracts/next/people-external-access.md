# People relationships and independent account administration

People owns employee work and the presentation of Company relationships. IAM remains the owner of external relationship metadata, accounts and access decisions in PostgreSQL. AD/UCS/Samba account, group and GPO integration is optional; this change does not establish that Samba is connected or enabled.

## People and Administration

People has three Company views: **Employees**, **External collaborators**, and **Unclassified**. Employees retain Company → Branch → Department navigation, positions, archive, search, roster and HR drawers. HR can create employees, positions and placements without an account, a prepared package or IT action. Employee creation, verified account linking and granting access are separate commands. Creating or moving a position never grants access automatically.

External organizations and independent affiliations are Company-owned IAM metadata, never Companies or organization nodes. External teams can name the internal Department they work with. Collaborators have an internal employee sponsor, status and relationship expiry. They can be recorded before any account exists. Neither affiliation nor team membership creates employment, positions, grants, levels or GPO inheritance.

Unclassified means a Company account or prepared invitation subject has neither an employee relationship nor an explicit external relationship. Account creation by IT does not classify a person as external. `classification: employee | external | unclassified` is a presentation property, independent of `accountSource: local | directory`. Verified aliases are collected into `linkedUserIds`; relationship cutoffs cover every linked identity in the selected Company.

Administration → Technology → Resources → Accounts is the account/invitation/grant editor. Roles, groups, access packages and position bindings keep their existing Administration editors. AD/GPO and Directory work stay in Technology. People profiles link to related accounts and account details link back to employee profiles or relationships. Detail and edit surfaces use right drawers. Search, classification and selected subject use URL state for reload and Back/Forward. Retained Users & Access and IAM links redirect to their owner while preserving filters, repeated query values and fragments.

## HR and IT authority

`people.relationships.read` and `people.relationships.manage` are Company-scoped permissions for the default HR administrator, IT administrator and Company owner. They add no account, invitation, role or Directory authority to HR. Relationship reads return basic people, organizations, teams and units; they omit assignments, role/permission definitions and Directory configuration. Relationship mutations allow only organization, team and subject metadata, including classifying an already verified Company account as external. They accept no account-link field and never match identities using a name or email alone.

HR status/expiry edits affect authorization immediately through `iam_principal_active`, including verified aliases, without a worker or IT confirmation. Recording a past expiry is a valid immediate cutoff. Renewal does not create grants, extend assignment deadlines or restore revoked assignments. Updating an invited relationship to Active cannot bypass acceptance and proof of identity. Self changes and protected recovery-account changes remain denied.

IT can prepare an unclassified account and custom roles before HR records exist. Account preparation creates only an IAM subject; invitation and access remain explicit later commands. An unclassified account can receive direct grants under the existing delegation rules. External invitations/grants require an affiliation, active employee sponsor and future relationship expiry. Existing local accounts must prove their password during acceptance; Directory accounts retain verified Directory linking. No HR profile is created by an invitation.

## API and command boundary

People prefix: `/api/v1/holdings/:holdingSlug/people/relationships`.

| Method and suffix | Operation |
| --- | --- |
| GET `/` | Relationship-only Company projection |
| POST `/preview` | `{change}` with organization, team or subject metadata |
| POST `/apply` | `{previewToken}`; revalidate and apply the reviewed relationship |

IAM's existing `/iam/people-access`, `/preview` and `/apply` support IT account preparation, roles, grants, invitations and revocation. Both transports use the original IAM owner transaction, normal authenticated work-context headers, same-origin checks, Idempotency-Key, fresh MFA, current assignment/Company authority, optimistic versions, immutable audit and receipts. HR cannot submit a privilege command through its transport or apply a privilege-bearing token. Previews expire after ten minutes and bind the actor, session, authorizing assignment and data fingerprint. Apply checks current authority and data again inside the serializable command.

IT retains its delegation ceiling, supported Company/Branch/Department grant scopes, immutable role revisions and self-access restrictions. Ownership, recovery and Platform permissions cannot be published through direct IT role authoring. Pending reviews remain independent and are never auto-approved. Position package revisions remain preparations for explicit individual access decisions.

## Migration and verification

Migrations `1042` and `1043` retain contractor/Finance history and historical delegation evidence. Additive migration `1072_people_relationship_authority.sql` publishes the two relationship permissions and extends the authorization predicate to retain invitation acceptance gating during HR edits. It creates no accounts, sends no invitations and grants no individual access. There is no Directory data migration or staging/production change in this development task.

Coverage lives in the identity-access People access unit/integration suites, People lifecycle/directory selection suites and workplace navigation suite. It covers IT-first account activation, HR-only metadata commands, filtered reads, Company boundaries, forged privilege previews, alias cutoffs, immediate expiry and renewal without assignment changes. UI verification covers the authenticated public development pages, desktop/mobile drawers, search and history. Development smoke checks are not full migration acceptance.
