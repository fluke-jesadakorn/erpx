# HR placement and Technology provisioning

Implemented 2026-09-18. This supersedes the placement/access coupling and reviewed-only Directory flow in [the original lifecycle contract](people-lifecycle.md). Existing reviewed operations retain their original approval method.

## User flow

People owns HR positions, employment, managers, effective dates and work requirements. A placement can start or transfer an employee without an access package, verified Directory account or completed IT preparation. It never grants access. HR Explorer contains people and position targets; access/resource editors live in Technology.

`/holdings/:holdingSlug/technology` provides Positions, Employees, Directory & GPO, and Resources. Old People access URLs redirect to these views. IT can prepare packages before HR creates positions or bind published packages afterward. One immutable package revision can serve multiple positions. Editing a position binding does not provision existing occupants automatically.

Employees supports selecting up to 100 people, previewing additions/removals, applying a position package, adding a separate supplemental package, exclusions, expiry and explicit withdrawal of a source. Applied package snapshots retain the actual revision, scope and authorizing assignment. Moving the employee in HR does not withdraw their old applied access. The display compares that snapshot with the current position binding.

Expiry, explicit revocation, suspension and offboarding still gate authorization. HR return does not restore access. An explicit IT restoration after an effective HR return revokes earlier grants permanently and prepares new access. Account disable/enable continues to require the existing identity and lifecycle safeguards.

Directory changes use direct **Preview → Apply**, `iam.directory.apply`, fresh MFA, Company scope and current worker-time authority. ERPX role grants from a package always create independent reviews; direct Directory approval cannot approve those roles. Another reviewer must possess grant authority and cannot approve a grant for themselves. New role grants remain unavailable until the package application is effective and review completes.

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
| `/api/v1/holdings/:slug/people/directory-operations` | `refresh`, `configure`, `preview`, `submit`, `apply`, `retry`, `cancel`; existing field-sync APIs |

Migrations `0056`–`0058` are additive. They preserve assignment IDs and old reviews, add applied snapshots, provisioning operations/grants and membership sources, and backfill eligible completed legacy cases without sending Directory writes. Authorization now uses applied snapshots instead of the employee's current HR position. Cache revision triggers and expiry fences include the new sources.

Directory operation steps record individual outcomes. Failed transient steps can resume under the same operation ID after current MFA/authority checks. Stale versions and uncertain provider writes require a new preview. Preparation tasks and independent role review status remain visible separately from Directory read-back.

See [verification evidence](../../testing/hr-it-technology-2026-09-18.md).
