# CONTEXT.md

Business language for ERPX, independent of implementation. People and spending terminology follows the [approved requirements](docs/plan/2026-09-14-spending-approval-requirements.md); delivery status is recorded there.

## Identity & scope

- **Platform Workspace** — Control-plane scope. Platform roles never inherit into or authorize customer Holding data.
- **Holding Workspace** — Customer-wide scope identified publicly by an immutable Holding slug and resolved internally to an opaque Holding ID.
- **Local Workspace** — A Branch or Department scope. Structural nesting records placement; it does not create a fourth authorization level.
- **Holding** — Top-level customer isolation boundary; owns a data pair. All business data is partitioned by Holding.
- **Company** — Legal and data-policy boundary inside a Holding. Selected within a Holding Workspace and derived from lineage in Local Workspaces; never a URL or assignable role level.
- **Product Brand** — ERPX's own visual identity. It identifies the software product and never identifies the customer or the legal issuer of a business document.
- **Workspace Brand** — Optional Holding or Branch visual identity shown in workspace context and customer documents. A Department uses its Branch brand when nested below a Branch, otherwise its Holding brand.
- **Document Brand Mode** — Per-print composition of Holding, Branch, or both Workspace Brands, constrained by the Branch derived from the business record. It does not change the Company that legally issues the document.
- **Visual Endorsement** — Co-display of Holding and Branch marks with structural relationship text. It communicates organizational affiliation, not approval, authenticity, or a digital signature.
- **Organization Node** — Authoritative Company, Headquarters, Branch, Department, or Team placement with lineage and an immutable parent-scoped slug.
- **Branch** — Local organization node representing a physical or logical site.
- **Department** — Local organization subtree, either directly under the Holding hierarchy or nested under a Branch.
- **WorkspaceScope** — Resolved Platform, Holding, Branch, or Department context carrying public slugs, opaque IDs, lineage, effective Company, and selected assignment.
- **AuthorityScope** — One of three authorization levels: Platform, Holding, or Local. Branch and Department are peers at the Local level.
- **Principal** — A distinct actor that can sign in and be authorized. Different login identities remain distinct Principals even when independently reviewed as belonging to the same Person.
- **Provider Identity** — A stable identity issued by a named directory authority, independent of changing contact details or usernames.
- **Directory Connection** — An optional, Company-approved source of authentication and membership evidence. Several may coexist with local accounts.
- **Role Revision** — An immutable, approved version of the permissions and delegation limits of a Business Role.
- **Grant Ceiling** — The bounded set of permissions an administrator may delegate within their own scope and validity; it does not grant access to the underlying business records.
- **Migration Batch** — A reviewed, fixed list of people whose local Company access will be replaced by explicitly approved directory access.
- **Directory Required** — A Company access policy requiring the approved directory Principal. Loss of directory membership never restores local access.
- **Access Decision** — An explanation of whether a person or role may use a capability in a scope, including its evidence and remaining conditions. A Conditional decision alone does not authorize an action.
- **Assignment** — Time-bounded grant of a scoped role to a principal; always expires. Holding grants inherit to descendants, Branch grants inherit to nested Departments, and Department grants stay within their subtree.
- **Business Role** — A set of application responsibilities granted through a scoped Assignment, distinct from a person's Position, Staff Level, or reporting relationships.
- **Local Access Group** — A Company-owned collection of People whose membership is maintained manually or by a Membership Rule. Nested membership does not change employment placement or grant access by itself.
- **Membership Rule** — A Company-defined set of conditions over a Person's Company attributes and active placement that determines automatic group membership.
- **Custom Attribute** — A Company-defined piece of information about a Person, Local Access Group, or Organization Node. Person values belong to the Company relationship, not every identity of the human.
- **Access Policy** — A named, versioned collection of Business Roles with explicit access scopes and expiry dates.
- **Policy Attachment** — Application of an Access Policy to a group or organizational subtree, with optional excluded subtrees. Its membership target is separate from the scope of the resulting access.
- **Derived Grant** — An independently reviewed Assignment that remains valid only while its policy source and the beneficiary's eligibility remain valid.
- **Break-Glass Session** — Emergency, time-boxed, dual-controlled elevation.

## Classification & audit

- **DataClassification** — Public < Internal < Confidential < Restricted; propagated via stricter-wins.
- **Audit Event** — Append-only record of who did what, when, for what purpose and correlation.
- **Global Read Audit** — Audited record of a cross-boundary read.

## Collaboration & reference

- **Referenceable** — Any record addressable at stable `/reference/item/:id` and linkable as `@<context>/<id>` (e.g., `@finance/inv-28-01-2026-1234`).
- **Typed Mention** — A mention stored as a typed edge, not free text — validated and auditable.
- **Collaboration Thread** — Thread of comments on a record, with reactions, following, and action cards.
- **Organization Channel** — A Company, Branch, or Department conversation whose membership follows active staff placement. Membership is distinct from seniority, approval authority, and membership of private conversations.
- **Field Suggestion** — Human-approved AI proposal for a field value; must be explicitly applied or rejected.
- **Workspace Record** — A module-owned business record with typed editable fields, an optimistic version, scoped placement, and an explicit review lifecycle. It is distinct from a read-only Sample Record.
- **Workflow Transition** — An attributable move between allowed Workspace Record states; review decisions require an eligible person other than the creator.

- **Record Reference** — A stable handle to any business record (`<context>/<id>`) resolvable at `/reference/item/:id`.
- **Reference Edge** — A typed, ordered, deduped, append-only link from a host record to a target `Record Reference` or `Principal`, with optional role label (`blocks`, `dependsOn`, `billedBy`, `fulfills`); soft cap 50 per host; backlink-queryable.
- **Field Patch** — A materialized `RecordFieldPatch` proposing field values for a host, built from pinned source versions, with `Evidence` per source and `Provenance`.
- **Evidence** — Pinned `FieldSuggestionEvidence.record { record, version }` proving the source snapshot used for a patch.
- **Provenance** — Provider/model/prompt digest describing how a suggestion was produced.
- **Conflicted** — Patch state when host or source version/digest drifted since the suggestion was computed; requires re-request or explicit conflict acceptance.

## Executive reporting (proposed: [AI Live Canvas](docs/plan/2026-09-18-executive-live-canvas.md))

- **Executive Report** — A creator-owned, private-by-default analysis of authorized Company information, arranged for ongoing use and optionally shared with named people.
- **Report Widget** — A movable part of an Executive Report that presents an analysis, explanation, or interaction with a defined data scope.
- **Report Share** — An explicit, revocable grant to a named recipient for a specified Executive Report or a selected part of it. It does not reveal the author's private conversation or grant general access to source modules.
- **Report Distribution Contract** — The bounded information a publisher is authorized to disclose through a shared report, including its permitted recipients, scope, and use. It is distinct from each recipient's independent access to the source records.

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

- **Roster Week** — A department’s plan of staff work shifts for a Monday–Sunday week in a named timezone, shared with staff when published.
- **Work Shift** — A dated interval of planned work with a required headcount; it may cross midnight.
- **Shift Template** — Reusable work times and headcount used to prepare a Work Shift. Changing a template does not change an existing shift.
- **Roster Assignment** — A Person’s planned participation in a Work Shift under a particular Engagement. It is distinct from an access Assignment.
- **Open Slot** — Unfilled headcount in a published Work Shift that eligible staff may request.
- **Roster Change Request** — A staff proposal to take an Open Slot, move an existing Roster Assignment, or exchange assignments with a named coworker.
- **Shift Swap** — An exchange of two staff members’ Roster Assignments, effective only after the coworker accepts and an independent roster manager approves.

- **Person** — A stable human identity in a Holding, independent of login access and any particular employment or contractor engagement.
- **Engagement** — A time-bounded relationship between a Person and a Company, describing worker type, organizational placement, Position, Staff Level, Reporting Manager, and status.
- **Retirement** — An HR-planned reason for ending a Person’s Engagement, with an explicit last working day and a separately confirmed access cutoff. It does not occur automatically from age or birthday.
- **Access Cutoff** — The confirmed instant when a Person’s Company access ends, independently of handover, financial clearance, or directory verification.
- **Retirement Clearance** — The attributable HR, handover, asset, directory, Payroll, and Finance checks associated with a retirement case. Completing a check records a review; it does not approve an obligation, post a journal, or release a payment.
- **Not Required** — A clearance result explaining why a particular check does not apply, recorded by its authorized owner.
- **Active Staff** — Employees and contractors with one current active Engagement and an active linked account in the Company. Personal requests and assigned work belong to staff independently of their selected business role.
- **Staff Level** — A Company-wide classification of seniority with a code, name, and level number; smaller numbers mean greater seniority, starting with Level 1 for CEO. An Engagement has one current level and retains its level-change history; a level alone grants no application permissions or approval authority.
- **Position** — A Company-defined job in an organizational unit, with a title, default Staff Level, and expected reporting Position. It exists independently of its occupants and may be vacant or shared by multiple staff.
- **Reporting Manager** — The named, more senior person to whom a staff member directly reports within the same Company. This relationship identifies the person's actual manager, separately from a Position's expected reporting relationship.
- **Organization Template** — A reusable description of organizational positions, default Staff Levels, and expected reporting relationships, independent of the people who occupy those positions.
- **Organization Chart** — A view of positions, occupants, vacancies, and actual reporting relationships within Company, Branch, and Department placement.
- **Department Head** — The named person responsible for a Department. Their responsibility is distinct from another staff member's direct Reporting Manager and from the approver selected for a particular request.
- **Approval Cycle** — One submitted version of a request or task phase, with an ordered route and attributable decisions. Rejection, correction, and resubmission retain earlier cycles.
- **Approval Step** — One named person's decision within an ordered approval route; only the current step is actionable.
- **Payment Request** — A staff request to pay an employee or supplier for a stated business purpose, supported by documents and independent approval before payment is recorded.
- **Spending Request** — A staff request for a purchase, expense reimbursement, or payment, with an attributable requester and spending purpose.
- **Approval Template** — A reusable Company policy defining the ordered review stages and participant eligibility for a Spending Request. It is distinct from a Routine Template that describes work to perform.
- **Approval Exception** — An explicit variation of an Approval Template for a named organizational Position or request type.
- **Approval Readiness** — The state in which a request has the organizational information and eligible, independent participants required for its approval and payment route.
- **Organizational Approval** — Review of a Spending Request by an eligible superior in the requester's reporting chain.
- **Accounting Approval** — Independent Accounting review of prepared spending details before the obligation proceeds through its remaining purchasing, posting, and payment work.
- **HR Review** — The final leave decision, made by an eligible HR administrator independent of both the requester and the Department approver.
- **Contractor** — A Person engaged under a services relationship whose approved obligation is handled through Finance payables rather than employee wages.
- **Compensation Revision** — An effective-dated, approved description of an Engagement's pay basis and components.
- **Payroll Run** — A versioned calculation for one pay group and period, with immutable input snapshots, results, approvals, and accounting links.
- **Country Pack** — A versioned jurisdiction rule set that must be validated and activated before production payroll is allowed.
- **Payment Batch** — An independently approved collection of outstanding obligations prepared for external payment-file export and later confirmation.
- **Payment Slip** — Bank payment evidence attached to a particular recorded payment. _Avoid_: Payslip, which describes employee pay rather than proof of bank payment.
- **Payslip** — An employee's payroll statement describing earnings, deductions, and net pay for a pay period.

## Leave

- **Leave Type** — A Company-defined category of absence with its own entitlement, evidence, and privacy rules.
- **Leave Policy** — An effective-dated set of leave entitlements and working-day rules offered by a Company to eligible Engagements.
- **Leave Policy Assignment** — The effective-dated association of one Engagement with one version of a Leave Policy. A Position supplies neither leave entitlement nor approval authority.
- **Leave Request** — A staff member's request for schedule-aware time away under the Leave Policy assigned to their active Engagement.
- **Leave Day Segment** — A full day, first half, or second half of one working date included in a Leave Request.
- **Leave Balance Entry** — An immutable change to an Engagement's entitlement, reservation, use, or reversal for one Leave Type.
- **Leave Cancellation Review** — The manager-and-HR review required to reverse approved leave; the absence and consumed entitlement remain effective until the review succeeds.

## Commercial & platform

- **Commercial Catalog Revision** — A version of a Hosted Support package and monthly fee, effective for new service periods without changing earlier agreements.
- **Entitlement** — Right to use a capability, allocation-booked.
- **Billing Lifecycle** — Progress of an issued invoice and its payment, with Stripe as the authority for payment outcomes.
- **Hosted Enrollment** — A Holding’s agreement to use Hosted services, with one Support package and month-end changes or termination.
- **Billing Account** — The single customer billing relationship covering every Company and Branch in a Holding.
- **Service Period** — A Bangkok calendar month of Hosted service, with Support prorated by calendar day in the first month.
- **Reviewed Actual Cost** — A verified expense paid by ERPX on behalf of a Holding, without markup and excluding customer-funded BYOK.
- **Cost Completeness** — An explicit confirmation that all known costs for a period were reviewed; absence of costs does not imply completeness.

## Operations & compliance

- **Directory Authority** — The one writable identity namespace for a domain, including its SID, secrets, LDAP objects and SYSVOL. A restored authority replaces its source; it is not a second independent writable copy.
- **Provider Identity Alias** — A verified one-to-one mapping from a provider's current immutable identifier to ERPX's existing Provider Identity, used to preserve links and audit history during a directory backend migration.
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

## Point of sale

- **POS Sale** — A customer bill with its own sale, payment and fulfillment progress. Splitting a bill allocates its quantities and amounts without repeating work already sent to a kitchen.
- **Register** — A branch's point of sale associated with a warehouse and a retail or restaurant workflow.
- **Cash Session** — A period of responsibility for a register's cash, beginning with a float and ending with a counted balance and independent review.
- **Fulfillment** — The goods or preparation work needed to satisfy sale items. One kitchen fulfillment may serve items allocated across several bills.
- **Reservation** — Stock committed to an unpaid checkout or queued kitchen work; available stock excludes these quantities.
- **Preparation Cost** — Ingredient value already consumed by cooking but not yet transferred to delivered food or waste.
- **Stock Return** — Physical receipt of resalable goods at their original cost. It is distinct from approval or completion of a refund.
- **Company Payment Connection** — The company's own Stripe merchant account used for customer payments, distinct from the account collecting ERPX subscriptions.
- **Payment Recovery** — Resolution of a provider-confirmed payment whose sale or accounting work remains incomplete.

## Hotel operations

**Property**: A hotel operated by one Branch and accounted for by its owning Company. A Company may operate several Properties.

**Hotel Reservation**: A confirmed booking that may contain several rooms, each with its own guest and stay status. It is distinct from a POS stock reservation.
_Avoid_: Stay, stock reservation.

**Stay**: One room's planned or actual occupancy for a guest, with an arrival, departure and history of assigned rooms.
_Avoid_: Hotel Reservation.

**Hotel Guest**: A visitor who books or occupies hotel accommodation, distinct from a member of the workforce.

**Folio**: A guest or payer's collection of hotel charges, payments, advances and adjustments. Several Folios may share one Hotel Reservation.
_Avoid_: Invoice; an invoice is an issued document summarizing a Folio.

**Room Readiness**: A room's eligibility for check-in after cleaning and independent inspection. Occupancy, cleanliness, maintenance closure and a guest's do-not-disturb request are separate facts.

**Business Date**: The hotel operating day currently open for transactions. It advances after the Night Auditor successfully closes that day, rather than automatically at midnight.

**Night Audit**: The auditor-initiated close of a Property's Business Date, recognizing each occupied room night and resolving outstanding operations before the next day opens.

**Stay Advance**: Money received toward accommodation or services before it is applied to charges. It is distinct from a damage deposit.

**Damage Deposit**: Refundable money held as security, applied to an agreed charge or returned to the guest.

**Group Allotment**: Room-type capacity held for a group until an agreed release date; picked-up reservations consume the held capacity once.

**Booking Quote**: A proposed room selection with nightly prices, inclusions and cancellation terms. It does not reserve inventory; availability and terms are checked again when the guest confirms.

**Published Hotel**: A Property deliberately made available for direct guest booking, with selected public rates and guest-facing descriptions.

**Free-Cancellation Deadline**: The last instant at which an eligible Hotel Reservation may be cancelled without a fee, recorded with the booking in the Property’s timezone. Later cancellations require hotel staff.

## Staff self-service

**Overtime Request**: An Engagement's Company-scoped request for authorization to work extra hours, or for review of hours already worked. Authorization of planned work does not approve worked hours.

**Overtime Actuals**: The dated interval and unpaid breaks actually worked, reviewed by the reporting manager and an independent HR reviewer. Correcting actuals restarts both reviews.

**Payroll Handoff**: The uniquely linked, auditable contribution of finally approved overtime hours to payroll preview. It supplies hours, not a pay rate, statutory calculation, or authority to pay.

**Leave Entitlement**: A Company policy's grant or accrual for an eligible Engagement and entitlement year. A Position supplies neither entitlement nor approval authority.
