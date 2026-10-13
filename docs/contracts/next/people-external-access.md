# People: employee directory and IT-owned external access

People is shared by HR and IT. HR's directory, archive, search, selected profiles, member pickers and lifecycle roster expose employee engagements only. IT's default Company-level entry is `/holdings/:slug/people?view=users`; `/people?view=internal` retains the employee organization view. IT receives basic employee identity and placement information, never compensation or restricted HR documents from its IT role.

## External relationships

External organizations and independent affiliations are Company-owned IAM metadata, not Companies or organization nodes. External teams may record the internal Department they work with. Neither team membership nor that Department relationship creates grants, employment, positions, levels or GPO inheritance.

An external subject can be prepared without a login or HR profile. New access requires an affiliation, active employee sponsor in the selected Company and a future relationship expiry. Access scope is selected independently from the collaboration Department. Classification is per Company. Invitations to an existing local account require its password before activating the Company relationship; names or matching contact details alone never verify a link. Existing directory accounts continue through verified Directory linking.

## Preview and direct application

`GET .../iam/people-access` returns the permission-filtered Company snapshot. `POST .../iam/people-access/preview` takes `{change}` and supports organization, team, subject, role, grant, invite and revoke operations. `POST .../iam/people-access/apply` takes `{previewToken}` with the normal rendered work-context headers and Idempotency-Key.

Previews expire after ten minutes and bind the actor, session, authorizing assignment and visible data. Apply checks the snapshot and current authority again inside a serializable IAM command. IT may publish custom roles, authorize invitations and assign access immediately with `iam.access.apply`; no HR action or independent reviewer is needed for these new commands. The delegation ceiling, supported Company/Branch/Department scopes, fresh MFA, optimistic versions, immutable role revisions, receipts and audit remain mandatory. Ownership, recovery and Platform permissions cannot be published through this flow. Self grants and publication of a role held by the author or a verified alias are refused.

Direct invitations reuse the normal activation flow and delivery jobs, with explicit authorization evidence separate from legacy reviews. A prospective assignment cannot authorize login while the invitation is pending. Activation rechecks the current relationship, sponsor, authorizing assignment and role revision; interrupted activation can resume. No external HR profile is created. Suspension and expiry gate principal authorization, including verified aliases, without relying on a worker.

## Compatibility

Migrations `1042_people_external_access.sql` and `1043_people_attribute_definition_authority.sql` retain contractor records and Finance history and classifies contractor-only people as external. Their legacy metadata is deliberately incomplete until IT supplies sponsorship and expiry; migration does not create accounts, send invitations or grant access. Existing delegation evidence on HR roles is retained to avoid invalidating legitimate descendant grants, while HR's IAM management/read permissions are removed. IT gains explicit apply authority and the bounded delegation ceiling. Field-definition/ACL administration has its own `iam.attributes.manage` authority, so default IT role authoring cannot open restricted HR fields. Existing explicit custom administrator delegations retain that authority. Existing pending reviews are never auto-approved and legacy reviewed operations retain their previous evidence.

Focused coverage lives in the identity-access People access unit/integration suites, employee directory/selection integration suites and workplace navigation suite. Development browser verification must cover desktop, mobile and right drawers. Production deployment and staging operations are outside this change.
