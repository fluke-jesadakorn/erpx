# CONTEXT.md

Business language for ERPX. Code-derived, implementation-free.

## Identity & scope

- **Platform Workspace** — Control-plane scope. Platform roles never inherit into or authorize customer Holding data.
- **Holding Workspace** — Customer-wide scope identified publicly by an immutable Holding slug and resolved internally to an opaque Holding ID.
- **Local Workspace** — A Branch or Department scope. Structural nesting records placement; it does not create a fourth authorization level.
- **Holding** — Top-level customer isolation boundary; owns a data pair. All business data is partitioned by Holding.
- **Company** — Legal and data-policy boundary inside a Holding. Selected within a Holding Workspace and derived from lineage in Local Workspaces; never a URL or assignable role level.
- **Organization Node** — Authoritative Company, Headquarters, Branch, Department, or Team placement with lineage and an immutable parent-scoped slug.
- **Branch** — Local organization node representing a physical or logical site.
- **Department** — Local organization subtree, either directly under the Holding hierarchy or nested under a Branch.
- **WorkspaceScope** — Resolved Platform, Holding, Branch, or Department context carrying public slugs, opaque IDs, lineage, effective Company, and selected assignment.
- **AuthorityScope** — One of three authorization levels: Platform, Holding, or Local. Branch and Department are peers at the Local level.
- **Principal** — An actor that can be authorized (human or service).
- **Assignment** — Time-bounded grant of a scoped role to a principal; always expires. Holding grants inherit to descendants, Branch grants inherit to nested Departments, and Department grants stay within their subtree.
- **Break-Glass Session** — Emergency, time-boxed, dual-controlled elevation.

## Classification & audit

- **DataClassification** — Public < Internal < Confidential < Restricted; propagated via stricter-wins.
- **Audit Event** — Append-only record of who did what, when, for what purpose and correlation.
- **Global Read Audit** — Audited record of a cross-boundary read.

## Collaboration & reference

- **Referenceable** — Any record addressable at stable `/reference/item/:id` and linkable as `@<context>/<id>` (e.g., `@finance/inv-28-01-2026-1234`).
- **Typed Mention** — A mention stored as a typed edge, not free text — validated and auditable.
- **Collaboration Thread** — Thread of comments on a record, with reactions, following, and action cards.
- **Field Suggestion** — Human-approved AI proposal for a field value; must be explicitly applied or rejected.
- **Workspace Record** — A module-owned business record with typed editable fields, an optimistic version, scoped placement, and an explicit review lifecycle. It is distinct from a read-only Sample Record.
- **Workflow Transition** — An attributable move between allowed Workspace Record states; review decisions require an eligible person other than the creator.

- **Record Reference** — A stable handle to any business record (`<context>/<id>`) resolvable at `/reference/item/:id`.
- **Reference Edge** — A typed, ordered, deduped, append-only link from a host record to a target `Record Reference` or `Principal`, with optional role label (`blocks`, `dependsOn`, `billedBy`, `fulfills`); soft cap 50 per host; backlink-queryable.
- **Field Patch** — A materialized `RecordFieldPatch` proposing field values for a host, built from pinned source versions, with `Evidence` per source and `Provenance`.
- **Evidence** — Pinned `FieldSuggestionEvidence.record { record, version }` proving the source snapshot used for a patch.
- **Provenance** — Provider/model/prompt digest describing how a suggestion was produced.
- **Conflicted** — Patch state when host or source version/digest drifted since the suggestion was computed; requires re-request or explicit conflict acceptance.

## Social engagement (paradigm: `docs/plan/04-social-paradigm.md`)

- **Post** — A record rendered as a feed card; the record stays truth, the post is its projection.
- **Feed** — Newest-first stream: Role Lane → Following → Hashtags → System. No engagement ranking.
- **Role Lane** — Pinned feed section for items needing your role (approvals, SLA breaches, period gates).
- **Follow** — Subscribe to record/person/hashtag; drives feed + notifications, never grants access.
- **Hashtag/Topic** — Scoped classifier (`#tag`); local / holding-moderated / system-reserved; placement-aware visibility.
- **Reaction** — Append-only signal (ack/seen); explicitly not a decision, never moves workflow.
- **Notification** — Mention/follow/lane event in the inbox; unread counts on lanes + mentions only.
- **Composer** — One box to post/comment/mention/tag across contexts; agent output enters as `proposed`.

## People & pay

- **Person** — A stable human identity in a Holding, independent of login access and any particular employment or contractor engagement.
- **Engagement** — A time-bounded relationship between a Person and a Company, describing worker type, placement, manager, role, and status.
- **Active Staff** — Employees and contractors with one current active Engagement and an active linked account in the Company. Personal requests and assigned work belong to staff independently of their selected business role.
- **Staff Level** — A Company-wide classification with a code, name, and rank; higher ranks indicate higher levels. An Engagement has one current level and retains its level-change history; a level grants no application permissions or approval authority.
- **Department Head** — The named person responsible for the first review of their Department's staff requests. Their own requests go to their reporting manager, or an independent Company-authorized owner at the top.
- **Approval Cycle** — One submitted version of a request or task phase, with an ordered route and attributable decisions. Rejection, correction, and resubmission retain earlier cycles.
- **Approval Step** — One named person's decision within an ordered approval route; only the current step is actionable.
- **HR Review** — The final leave decision, made by an eligible HR administrator independent of both the requester and the Department approver.
- **Contractor** — A Person engaged under a services relationship whose approved obligation is handled through Finance payables rather than employee wages.
- **Compensation Revision** — An effective-dated, approved description of an Engagement's pay basis and components.
- **Payroll Run** — A versioned calculation for one pay group and period, with immutable input snapshots, results, approvals, and accounting links.
- **Country Pack** — A versioned jurisdiction rule set that must be validated and activated before production payroll is allowed.
- **Payment Batch** — An independently approved collection of outstanding obligations prepared for external payment-file export and later confirmation.

## Commercial & platform

- **Commercial Catalog Revision** — Versioned commercial catalog mirrored from Stripe.
- **Entitlement** — Right to use a capability, allocation-booked.
- **Billing Lifecycle** — State machine of a billing attempt, Stripe-authoritative.

## Operations & compliance

- **Project** — A creator-owned collection of Company work with explicitly managed staff membership.
- **Staff Task** — A piece of operational or Project work with one accountable assignee, a due date, supporting attachments, and ordered approvals before work, after completion, or both.
- **Completion Note** — The assignee's account of the work delivered for completion review.
- **Task Revision** — An explicit, explained material change to a submitted task that returns it to draft and restarts its approvals while retaining prior decisions.
- **Rework** — Work returned to the assignee after completion review rejects the submitted result.
- **Routine Template** — A reusable Company description of work, default assignment, and approval phases; staff may run or clone a published template.
- **Routine Schedule** — A daily, weekly, or monthly instruction to create independent occurrences of a Routine Template within a timezone and date range. Paused periods produce no occurrences.
- **Routine Occurrence** — An independent Staff Task created from the template version current when the occurrence is generated, with its own due date and approval history.
- **Blocked Route** — A request or task whose required placement or eligible approver configuration needs correction before submission or decision.
- **Outbox Event** — Transactional event emitted with the write that produced it.
- **Projection** — Rebuildable read model derived from outbox events.
- **Holding Provisioning** — Platform workflow that reserves and activates a holding’s data pair.
- **PITR** — Point-in-time recovery via WAL archive.
