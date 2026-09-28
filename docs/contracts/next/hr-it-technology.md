# HR placement and Technology provisioning

Implemented 2026-09-18. This supersedes the placement/access coupling and reviewed-only Directory flow in [the original lifecycle contract](people-lifecycle.md). Existing reviewed operations retain their original approval method.

## User flow

People owns HR positions, employment, managers, effective dates and work requirements. A placement can start or transfer an employee without an access package, verified Directory account or completed IT preparation. It never grants access. HR Explorer contains people and position targets; access/resource editors live in Technology.

HR may create and assign an active Position whether its optional GPO mapping is absent, pending, or blocked. People shows GPO progress as a separate, nonblocking note; employment readiness is determined only by HR placement rules. An in-flight legacy access case may be superseded by an HR-only placement before it takes effect, with its prior reviews and history retained.

`/holdings/:holdingSlug/technology?view=organization` provides Organization & Access: one searchable organization/Level/Directory explorer, contextual details, and a GPO library without feature tabs. The former Positions, Employees and Directory & GPO URLs retain their selected records in this workspace. Resources and Custom attributes remain separate destinations. IT can prepare packages before HR creates positions or bind published packages afterward. One immutable package revision can serve multiple positions. Editing a position binding does not provision existing occupants automatically.

Employees supports selecting up to 100 people, previewing additions/removals, applying a position package, adding a separate supplemental package, exclusions, expiry and explicit withdrawal of a source. Applied package snapshots retain the actual revision, scope and authorizing assignment. Moving the employee in HR does not withdraw their old applied access. The display compares that snapshot with the current position binding.

Expiry, explicit revocation, suspension and offboarding still gate authorization. HR return does not restore access. An explicit IT restoration after an effective HR return revokes earlier grants permanently and prepares new access. Account disable/enable continues to require the existing identity and lifecycle safeguards.

Directory changes use direct **Preview → Apply**, `iam.directory.apply`, fresh MFA, Company scope and current worker-time authority. ERPX role grants from a package always create independent reviews; direct Directory approval cannot approve those roles. Another reviewer must possess grant authority and cannot approve a grant for themselves. New role grants remain unavailable until the package application is effective and review completes.

## Position GPO mapping

Organization & Access extends direct Position mappings with versioned Level and Department rules. A Position receives the union of rules for its Company-wide default Level, its Department and its own ID; an occupant's independently assigned Staff Level does not select a different GPO. New matching Positions and later effective placements inherit the current rules. The UI displays all sources, coalesces duplicate policy targets, and directs inherited edits to their source. Removing a direct rule leaves inherited sources effective. There is no per-Position inheritance suppression or Department-specific Level selector in this iteration. Existing Directory link order and enforcement remain authoritative.

Drag-and-drop and equivalent keyboard/button actions always open the same impact review. Rule publication includes the relevant organization, rule and Directory versions. Jobs and membership claims retain stable rule identities so removing one rule cannot remove memberships still required by another rule or a manual/package source. Legacy direct mappings, revisions, jobs and owned settings are migrated without issuing Directory writes. Department replacement carries its rules forward transactionally.

Company IT publishes an optional mapping from an existing GPO to a Position, a user filter group, and an explicit computer target OU. IT may publish before HR assigns anyone or after the Position already has occupants, and may later remove the mapping with an impact review. A holder without a verified account does not prevent publication; that holder's Directory work waits for account linkage. Publication, removal, and effective HR placement lead to automatic reconciliation of the mapped GPO for current, active holders. This is Directory policy work, not an automatic ERPX Business Role grant or an access-package application. No IT decision is required for HR placement.

A shared GPO uses the union of filter groups and target OU links from its current Position mappings. ERPX tracks the settings it owns and preserves other GPO settings and membership sources. Transfer, departure, remapping, and account changes remove a Position's membership only when no other current source still requires it. Directory read-back confirms the server-side change; it does not prove Windows endpoint application.

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
| `/api/v1/holdings/:slug/technology/gpo-rules` | GET rules/effective policies; `preview`, `publish`, `remove-preview`, `remove`, `retry-config` |
| `/api/v1/holdings/:slug/people/organization/position-moves` | GET status; `preview`, `submit`, `:id/cancel` |
| `/api/v1/holdings/:slug/people/directory-operations` | `refresh`, `configure`, `preview`, `submit`, `apply`, `retry`, `cancel`; existing field-sync APIs |

Migrations `0056`–`0058` are additive. They preserve assignment IDs and old reviews, add applied snapshots, provisioning operations/grants and membership sources, and backfill eligible completed legacy cases without sending Directory writes. Authorization now uses applied snapshots instead of the employee's current HR position. Cache revision triggers and expiry fences include the new sources.

Migrations `0320`–`0321` introduce versioned organization GPO rules, immutable revision snapshots, expiring actor-bound previews, source-specific membership claims and scheduled atomic Position moves. Existing direct mappings retain their legacy API and ownership evidence. Rule previews expire after ten minutes and are checked against the current organization, accounts, Directory targets and rule revisions before publication. The organization read model omits compensation; `effectiveByPosition` exposes effective policy arrays with every inherited source, while employee lifecycle summaries aggregate delivery across all applicable connections.

Directory operation steps record individual outcomes. Failed transient steps can resume under the same operation ID after current MFA/authority checks. Stale versions and uncertain provider writes require a new preview. Preparation tasks and independent role review status remain visible separately from Directory read-back.

See [original verification evidence](../../testing/hr-it-technology-2026-09-18.md) and [Organization & Access verification](../../testing/organization-access-2026-09-26.md).
