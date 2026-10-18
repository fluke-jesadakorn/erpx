# HR placement and Technology provisioning

The shared People menu now also exposes [IT-owned Users & Access](people-external-access.md), including external organizations and teams without employee profiles. Its direct role/invitation/grant flow is separate from the legacy reviewed package operations described below. HR placement remains employee-only and never grants access.

Implemented 2026-09-18. This supersedes the placement/access coupling and reviewed-only Directory flow in [the original lifecycle contract](people-lifecycle.md). Existing reviewed operations retain their original approval method.

## User flow

People owns HR positions, employment, managers, effective dates and work requirements. A placement can start or transfer an employee without an access package, verified Directory account or completed IT preparation. It never grants access. HR Explorer contains people and position targets; access/resource editors live in Technology.

HR may create and assign an active Position whether its optional GPO mapping is absent, pending, or blocked. People shows GPO progress as a separate, nonblocking note; employment readiness is determined only by HR placement rules. An in-flight legacy access case may be superseded by an HR-only placement before it takes effect, with its prior reviews and history retained.

`/holdings/:holdingSlug/technology?view=organization` provides Organization & Access: one searchable organization/Level/Directory explorer, contextual details, and a GPO library without feature tabs. The former Positions, Employees and Directory & GPO URLs retain their selected records in this workspace. Resources and Custom attributes remain separate destinations. IT can prepare packages before HR creates positions or bind published packages afterward. One immutable package revision can serve multiple positions. Editing a position binding does not provision existing occupants automatically.

Employees supports selecting up to 100 people, previewing additions/removals, applying a position package, adding a separate supplemental package, exclusions, expiry and explicit withdrawal of a source. Applied package snapshots retain the actual revision, scope and authorizing assignment. Moving the employee in HR does not withdraw their old applied access. The display compares that snapshot with the current position binding.

Expiry, explicit revocation, suspension and offboarding still gate authorization. HR return does not restore access. An explicit IT restoration after an effective HR return revokes earlier grants permanently and prepares new access. Account disable/enable continues to require the existing identity and lifecycle safeguards.

Directory changes use direct **Preview → Apply**, `iam.directory.apply`, fresh MFA, Company scope and current worker-time authority. ERPX role grants from a package always create independent reviews; direct Directory approval cannot approve those roles. Another reviewer must possess grant authority and cannot approve a grant for themselves. New role grants remain unavailable until the package application is effective and review completes.

## Position GPO mapping and task

Creating an active Position creates a distinct Position GPO task in Organization & Access. The task starts pending. IT can assign multiple existing GPOs directly to the Position using Configure → Preview → Confirm; every mapping requires a user security filter group and computer target OU. Level and Department are no longer GPO rule targets and moving a Position does not inherit new policies. Existing Level and Department rules are expanded into direct Position rules for their effective Positions during migration. Old rule revisions and membership ownership remain attributable, and migration sends no Directory writes.

The task is blocked if any mapped configuration is blocked. It is completed when at least one mapping exists and every active mapped configuration has verified Directory server read-back. Employee account linkage and membership delivery are tracked separately, as is Windows endpoint application. A task with no mapping can be closed as **GPO not required** with a reason and expected version. A material Position change or a new mapping reopens that disposition. Package preparation remains a separate activity.

HR may create and assign people or move Positions while the task is pending or blocked. IT can edit person custom attribute values in its Company through `people.custom_attributes.write`, including fields whose custom write roles would otherwise exclude IT. It cannot use this permission to edit HR system fields, custom attribute definitions, or another Company's records. The GPO editor links to the supported settings editor. Rule publication, removal and retries retain Directory authority and fresh MFA checks.

A shared GPO uses the union of filter groups and target OU links from its current Position mappings. ERPX preserves unrelated GPO settings and membership sources. Directory read-back verifies server configuration; it does not prove application on a Windows endpoint.

## Occupied Position moves

Dragging a Position onto a Department or Level creates one previewed, effective-dated move for that Position and its reviewed non-ended Engagements. Department-only moves preserve actual Staff Levels. A Level move changes all occupants' actual levels, records their histories and applies the existing Level leave-default rules with their effective dates shown in preview; it never changes compensation automatically. Existing employment statuses and dates remain unchanged.

The move validates both the expected Position reporting graph and actual employee-manager graph, including affected reports, CEO constraints, destination availability and overlapping HR changes. Invalid managers require explicit replacement. The due worker rechecks current authority and the exact reviewed cohort and applies all HR changes, history and outbox events in one transaction. A stale or conflicting move blocks as a whole; it never silently includes newly assigned occupants. Pending moves may be cancelled. Directory reconciliation starts only after the HR change takes effect.

Moving an individual employee retains the existing HR placement workflow. Dropping on a Department requires a valid target Position. HR and IT command permissions remain independent, including Company scope and fresh MFA for Directory publication.

## Console and provider

The Directory console has a toolbar, OU tree, object table and properties panel. Group membership is separate from OU placement. Account/computer/group membership, nested groups, OU moves, GPO links and link order support drag previews and equivalent buttons. Group/OU cycles and cross-Company authority violations are rejected. Preview expands shared GPO impact through linked OUs, groups, people and computers. Computer policy requires an explicit target.

UCS accounts, groups and OUs use UDM with verified TLS and permanent IDs. GPO publication uses the [UCS Samba runner](../../../ops/ucs/README.md), Samba LDAPS, Python bindings and SYSVOL. Finite settings cover USB, inactivity lock, user wallpaper, Drive and Printer preferences, enabled/enforced links, order, block inheritance and security filtering. Updates preserve unsupported registry values, XML nodes/attributes, extension declarations and other GPT files.

The runner limits connection IDs, OU subtrees and owned/approved GPOs. Verified UDM/Samba identity mappings are required; reconcile them after object creation, movement or replication. Each operation has an immutable request fingerprint, backup, journal and read-back. A stale or uncertain operation requires a fresh inventory and preview. A transport retry reuses the operation ID and skips verified steps. UI results distinguish **server write verified** from **Windows endpoint applied**; endpoint confirmation is not inferred.

**UCS populated-container limitation:** UCS 5.2's UDM subtree move copies/recreates the container, changing its permanent identity. ERPX refuses that move before writing. Move the children out, move the empty OU, then move the children back using explicit previews. The provider also checks LDAP `hasSubordinates` immediately before writing, including children outside the console's supported object types. Asynchronous UDM moves follow approved progress links before read-back. Identity changes or an uncertain partial result stop further writes.

Filesystem share creation and resource ACL editing are outside this iteration. GPT permissions are handled as part of GPO publication. Windows/RSAT and endpoint policy application remain deferred.

## API and persistence

All mutations retain authenticated work contexts, origin checks, idempotent command receipts, optimistic versions, audit and outbox events.

| API prefix | Endpoints |
| --- | --- |
| `/api/v1/holdings/:slug/people/lifecycle` | `placements/preview`, `placements/draft`, `placements/submit`, `placements/:id/cancel`; existing package/position/task APIs |
| `/api/v1/holdings/:slug/technology` | GET workspace/status; `provisioning/preview`, `provisioning/apply`, `provisioning/retry`; Directory forwarding |
| `/api/v1/holdings/:slug/technology/organization` | GET permission-filtered organization tree and HR action capabilities |
| `/api/v1/holdings/:slug/technology/gpo-rules` | GET rules/effective policies; `preview`, `publish`, `remove-preview`, `remove`, `retry-config`, `not-required` |
| `/api/v1/holdings/:slug/people/organization/position-moves` | GET status; `preview`, `submit`, `:id/cancel` |
| `/api/v1/holdings/:slug/people/directory-operations` | `refresh`, `configure`, `preview`, `submit`, `apply`, `retry`, `cancel`; existing field-sync APIs |

Migrations `0056`–`0058` are additive. They preserve assignment IDs and old reviews, add applied snapshots, provisioning operations/grants and membership sources, and backfill eligible completed legacy cases without sending Directory writes. Authorization now uses applied snapshots instead of the employee's current HR position. Cache revision triggers and expiry fences include the new sources.

Migrations `0320`–`0321` introduce versioned organization GPO rules, immutable revision snapshots, expiring actor-bound previews, source-specific membership claims and scheduled atomic Position moves. Existing direct mappings retain their legacy API and ownership evidence. Rule previews expire after ten minutes and are checked against the current organization, accounts, Directory targets and rule revisions before publication. The organization read model omits compensation; `effectiveByPosition` exposes effective policy arrays with every inherited source, while employee lifecycle summaries aggregate delivery across all applicable connections.

Directory operation steps record individual outcomes. Failed transient steps can resume under the same operation ID after current MFA/authority checks. Stale versions and uncertain provider writes require a new preview. Preparation tasks and independent role review status remain visible separately from Directory read-back.

See [original verification evidence](../../testing/hr-it-technology-2026-09-18.md) and [Organization & Access verification](../../testing/organization-access-2026-09-26.md).

## Shared console presentation

People settings is a full workspace with authorized topics (Staff levels, Leave & overtime, Holiday calendars, Pay groups, Organization templates, Lifecycle handoff), list content and right-hand form drawers. Existing `people?panel=settings` links redirect without losing repeated filters, unit context or the policy editor.

Technology uses `view=organization|resources|directory|settings`. Organization & Access shares the People Units tree and grouped Position/People list; selected details retain Policies, Directory and Delivery. Resources has `section=packages|accounts|groups|policies|tasks`, Directory has `section=objects|connections|mappings|work`, and Access settings has `section=attributes|roles|matrix|groups|reviews`. Their defaults are packages, objects and attributes. Directory configuration/objects/mappings/jobs have a single owner destination. Former People IT settings and `view=attributes` links redirect with their context intact. Selections in organization, resource lists, Roles and Directory connections remain in the URL, including repeated filters and return context, so Back/Forward and reload restore the selected record. Existing provider, HR placement, GPO, MFA, review, idempotency and version rules remain unchanged. Shared presentation receives only the existing permission-filtered directory facts.

Detail panels dock above 1250px and become drawers on narrower viewports; phones at 650px use the full width. Navigation becomes a picker at 1024px. Long forms always use the shared form drawer, with retained drafts, visible command errors and focus return.


## Projects worker preparation foundation — implementation in progress

The [worker preparation decision](../../adr/ADR-047-source-bound-worker-preparation-foundation.md) preserves the HR-only placement boundary above. A closed Projects foundation may prepare or explicitly link one selected existing Company account through the original Directory owner, then use original placement Preview/Submit and its due algorithm. Current Company HR and Organization authority, source purchase/release, native account uniqueness and all original organizational/version rules remain required. It creates no accounts, access assignments, grants, access recipes or Project memberships. An already submitted exact HR-only placement settles its reviewed work during draining/expiry and remains a concrete withdrawal obligation; generic incoming jobs and overdue cancellation rules remain unchanged. This paragraph records scope for the in-progress implementation, not a current installed or acceptance receipt.

The Projects worker preparation source now supplies closed Accounts and explicit unlinked-Person selectors, original Directory create/link, HR-only placement Preview/Submit, original future cancellation, and source-bound durable due processing. Its current Company HR/Organization assignment and exact record/version/purchase/release remain mandatory. A successful self-Directory command may change only its own IAM Person projection from null to the exact original receipt/source-proven Person after deferred effects; all other authority fields remain exact. Target membership loss blocks new work and due application, while the original retained Person source permits authorized history/future cancellation. Source1063 is additive and not installed by this source batch.

## Learning learner preparation — source implementation

Learning's included `people.learner-reference` now has the closed named `knowledge/learning/settings/learners[/personId]` workflow and `people/learning-worker-startup` API. Existing Company accounts and explicit unlinked Persons are selected by current Company HR and Organization authority, then original Directory and HR-only placement establish eligibility for original Learning assignments. The constructor requires Learning and People release readiness and `learn.assignments` plus learner-reference purchase, without a full People or Projects purchase. Generic Directory remains separately protected. Distinct source1064 startup/placement relations, original due processing and terminal withdrawal evidence preserve the same original identity and current-authority boundaries as Projects1063. Reviewed due work may settle while Learning drains; new preparation cannot. Account creation, grants, access recipes, compensation, Payroll and automatic course assignment are excluded. Source installation and authenticated browser acceptance remain separate.
