# CONTEXT.md

Business language for ERPX, independent of implementation. Current delivery scope and status belong in the [production plan](docs/production-plan.md).

## Pilot acceptance

**Account**: A Company-owned CRM relationship with a business or individual, which may be a prospect before Finance recognizes a customer. A reviewed link to a Finance customer keeps the commercial and accounting identities distinct.

**Prospect**: An Account or Contact under consideration before a reviewed Finance customer relationship exists.

**Contact**: A person known to CRM within an Account, with independently recorded Sales and Support interactions. A shared name or email address does not prove that two Contacts are the same person.

**Guest Link**: A reviewed relationship between a CRM Contact and a PMS guest. PMS remains the owner of guest identity, reservations, stays, and folios.

**Company Setup Acceptance**: Independent confirmation that a Company's organization and staff placement are ready. It does not certify transaction correctness or authorize launch.

**Release Acceptance**: Independent, evidence-backed confirmation that the agreed delivery scope is ready for a named Company and software release with its reviewed configuration. Material changes require a new review.

**Readiness Checkpoint**: A dated review of completed evidence, remaining work and external blockers. It is not a commitment to open the system on that date.

**Thai Hotel Pilot**: The first operational release for one Thai hotel with its restaurant, purchasing, HR, Thai employee payroll and human-confirmed AI assistance. Contractors remain payable through Finance, separately from employee payroll.

## Identity & scope

- **Platform Context** — Control-plane authority for customer administration and service operations. Platform roles never inherit into or authorize a Holding's business data.
- **Holding Workspace** — Organization-wide scope identified publicly by an immutable Holding slug and resolved internally to an opaque Holding ID.
- **Local Workspace** — A Branch or Department scope. Structural nesting records placement; it does not create a fourth authorization level.
- **Holding** — Top-level organization and business-data isolation boundary. A Holding may be a customer or ERPX itself.
- **Customer Holding** — A customer organization served and billed by ERPX.
- **ERPX Holding** — ERPX's internal organization, with its own business records and Company; it is not a Customer Holding.
- **ERPX Management** — The ERPX Holding's module for customer and Platform administration. Access requires the selected ERPX Holding role to permit the module and an active Platform Assignment; the selected Platform role determines which operations are allowed.
- **Company** — Legal and data-policy boundary inside a Holding. Selected within a Holding Workspace and derived from lineage in Local Workspaces; never a URL or assignable role level.
- **Product Brand** — ERPX's own visual identity. It identifies the software product and never identifies the customer or the legal issuer of a business document.
- **Workspace Brand** — Optional Holding or Branch visual identity shown in workspace context and customer documents. A Department uses its Branch brand when nested below a Branch, otherwise its Holding brand.
- **Storefront App Brand** — A Company-owned short name, logo, and accent color for its installable POS and PMS apps. It identifies the daily-work app and never changes the Company's legal identity or the ERPX product brand.
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

## Files & publication

- **File Source** — One immutable original file whose identity follows its content. Possessing or naming it does not by itself grant access.
- **File Set** — The files collected for one Source Record, line, action, event, message, or publication purpose under one reviewed policy.
- **File Link** — An authorized association between a File Source and one File Set. Another record may reference the same source only through its own link.
- **Bound File** — A File Link fixed to a submitted record version or completed business action. It cannot be withdrawn or replaced in place.
- **Media Publication** — An approved public presentation derived from a private File Source, with its own description, ordering, and revision. _Avoid_: Public original.

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

## Executive reporting (proposed: [AI Live Canvas](docs/archive/planning-2026-09/2026-09-18-executive-live-canvas.md))

- **Executive Report** — A creator-owned, private-by-default analysis of authorized Company information, arranged for ongoing use and optionally shared with named people.
- **Report Widget** — A movable part of an Executive Report that presents an analysis, explanation, or interaction with a defined data scope.
- **Report Share** — An explicit, revocable grant to a named recipient for a specified Executive Report or a selected part of it. It does not reveal the author's private conversation or grant general access to source modules.
- **Report Distribution Contract** — The bounded information a publisher is authorized to disclose through a shared report, including its permitted recipients, scope, and use. It is distinct from each recipient's independent access to the source records.

## Daily work and social engagement

- **Today** — One personal entry point for the current day, actions needing the person, work being tracked, and authorized team updates. It is a view across owners, not the owner of a business transaction. _Avoid_: Home, Work as separate starting places.
- **Source Record** — The business item owned by its specialist application, such as an Expense Claim, Leave Request, or Purchase Request. Its owner controls its status, permissions, decisions, and evidence even when Today shows it.
- **Work Item** — A current, permission-checked view of one Source Record's next step and responsible person or team. It is not a copied task or a second workflow.
- **Daily Plan** — A person's dated ordering and time allocation for their own work. Moving an item in this plan does not change its deadline, roster, attendance, or approval state.
- **Post** — A person's update to an explicitly chosen audience, optionally linked to one or more Source Records. Editing the update does not edit those records.
- **Post Audience** — The people or organizational scope a poster chooses for one publication; it is bounded by each linked Source Record's own access rules. _Avoid_: Inherited audience from the previous post.
- **Feed** — Chronological stream of posts visible in the chosen context. Actionable Work Items have their own priority space so new posts do not bury required work.
- **Role Lane** — The separate priority space for items needing your role (approvals, SLA breaches, period gates).
- **Follow** — Subscribe to record/person/hashtag; drives feed + notifications, never grants access.
- **Hashtag/Topic** — Scoped classifier (`#tag`); local / holding-moderated / system-reserved; placement-aware visibility.
- **Reaction** — Append-only signal (ack/seen); explicitly not a decision, never moves workflow.
- **Notification** — A staff Inbox delivery about work, calendar activity, a mention, a direct notice, or a system effect. Its read and archive state belongs to the recipient and never decides the source workflow or grants access to it.
- **Inbox Event** — One meaningful source occurrence that may be delivered to several staff accounts in the same Company.
- **Inbox Delivery** — One recipient's copy of an Inbox Event, with independent read and archive state.
- **Staff Notice** — An immutable plain-text message sent by an active staff account to named colleagues or an authorized Company, Branch, or Department audience.
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
- **Position** — A Company-defined job in an organizational unit, with a title, default Staff Level, expected reporting Position, and whether its occupants require a weekly Roster. It exists independently of its occupants and may be vacant or shared by multiple staff.
- **Roster-required Position** — A Position whose active occupants must receive at least one assignment in each working week, except when approved leave covers every working day. This obligation is separate from Business Role and pay type.
- **GPO Rule** — An IT-owned association of a Directory Group Policy and its targets with a Company-wide Staff Level, Department, or Position. Level rules follow the Position's default level; Department rules follow the Position's placement.
- **Effective Position GPOs** — The combined Directory policies inherited from a Position's Level and Department and assigned directly to that Position. Every contributing rule remains attributable, even when several sources require the same policy.
- **Position GPO Mapping** — A GPO Rule assigned directly to a Position, independently of its inherited rules. It governs Directory policy work for active occupants independently of HR placement and ERPX Business Roles.
- **Position Move** — A scheduled, reviewed change that moves a Position and all its current occupants together. A Department move preserves individual Staff Levels; a Position Level change changes all occupants' Staff Levels with their histories.
- **Position Pay Default** — A versioned monthly pay recommendation for a Position and pay group. It is distinct from an Engagement's Compensation Revision and never determines Payroll by itself.
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
- **Compensation Draft** — A proposed Compensation Revision awaiting an independent approver; it is not current pay or a Payroll input.
- **Payroll Run** — A versioned calculation for one pay group and period, with immutable input snapshots, results, approvals, and accounting links.
- **Country Pack** — A versioned jurisdiction rule set that must be validated and activated before production payroll is allowed.
- **Payment Batch** — An independently approved collection of outstanding obligations prepared for external payment-file export and later confirmation.
- **Payment Slip** — Bank payment evidence attached to a particular recorded payment. _Avoid_: Payslip, which describes employee pay rather than proof of bank payment.
- **Payslip** — An employee's payroll statement describing earnings, deductions, and net pay for a pay period.

## Leave

- **Leave Type** — A Company-defined category of absence with its own entitlement, evidence, and privacy rules.
- **Leave Policy** — An effective-dated set of leave entitlements and working-day rules offered by a Company to eligible Engagements.
- **Leave Policy Assignment** — The effective-dated association of one Engagement with one version of a Leave Policy. A Position supplies neither leave entitlement nor approval authority.
- **Level Leave Default** — A versioned recommendation linking a Staff Level to a Leave Policy version. An Engagement receives entitlement through its Leave Policy Assignment, including when HR accepts this recommendation.
- **Leave Request** — A staff member's request for schedule-aware time away under the Leave Policy assigned to their active Engagement.
- **Leave Day Segment** — A full day, first half, or second half of one working date included in a Leave Request.
- **Leave Balance Entry** — An immutable change to an Engagement's entitlement, reservation, use, or reversal for one Leave Type.
- **Leave Cancellation Review** — The manager-and-HR review required to reverse approved leave; the absence and consumed entitlement remain effective until the review succeeds.

## Finance

- **Finance Workbench** — A role-aware list of accounting work that needs attention, ordered by business consequence and due date. It is distinct from a dashboard of record counts.
- **Accounting Period** — A Company-owned date interval that gates posting and close. A Group Reporting Period references Company periods without changing them.
- **Close Checklist** — The attributable set of reconciliations, evidence, owners, and blockers required before an Accounting Period may close.
- **Group Reporting Period** — A Holding reporting boundary that snapshots approved Company balances, ownership, policies, exchange rates, and consolidation adjustments for one reproducible consolidation run.
- **Consolidation Run** — One versioned calculation of Group financial statements for a Group Reporting Period and presentation currency. It never edits Company ledgers.
- **Group Account** — A reporting account in the Holding chart to which Company accounts are effective-dated mapped for consolidation.
- **Ownership Interest** — An effective-dated economic and control relationship between Holding entities, including the approved accounting treatment. Percentage ownership alone does not decide consolidation treatment.
- **Intercompany Pair** — Two independently posted Company records identified as opposite sides of the same Group transaction.
- **Consolidation Adjustment** — A versioned Group-only entry for eliminations, currency translation, non-controlling interests, equity accounting, or acquisition accounting. _Avoid_: Group journal, which suggests a Company-ledger posting.
- **Finance Proposal** — A human-reviewed suggestion derived from pinned record versions and evidence. It may prepare or explain work but cannot approve, post, pay, reconcile, or close.
- **Finance Country Pack** — A versioned jurisdiction rule set for tax, statutory documents, and filing exports. Activation requires accountable review and does not itself certify regulatory compliance.

## Commercial & platform

- **Commercial Catalog Revision** — An effective-dated version of Hosted pricing. The historical Support catalog and target system-plus-usage catalog remain distinct so past periods keep their agreed terms.
- **Entitlement** — Right to use a capability, allocation-booked.
- **Billing Lifecycle** — Progress of an issued invoice and its payment, with Stripe as the authority for payment outcomes.
- **Hosted Enrollment** — A Holding’s agreement to use Hosted services, with one Support package and month-end changes or termination.
- **Billing Account** — The single customer billing relationship covering every Company and Branch in a Holding.
- **Service Period** — A Bangkok calendar month of Hosted service, with the first month's system fee prorated by calendar day.
- **Measured Usage** — A source-linked observation of AI provider units or original-file byte-time attributed to the Holding and, when provable, its Company and Branch. It is not a charge.
- **Verified Provider Cost** — A successful ERPX-funded AI request's provider expense with currency and statement evidence. It becomes rateable only after the complete provider statement reconciles.
- **Rated Charge** — An immutable result of applying one effective price revision to complete measured usage and verified costs. It is not an issued invoice.
- **Cost Completeness** — An explicit confirmation that all known meters, provider costs, statement reconciliations, and approved extras for a Service Period were reviewed; absence of records does not imply completeness.
- **Sandbox Finance Ledger** — The isolated test Company ledger in ERPX Holding that mirrors confirmed Stripe Sandbox receivables, revenue, receipts, and reversals without entering real Company reports.

## Projects, routines & compliance

- **Projects & Routines** — Coordination of Project and recurring work across Company departments, with accountable owners, handoffs, and resolution of blockers. _Avoid_: Operations as a catch-all menu name.
- **Directory Authority** — The one writable identity namespace for a domain, including its SID, secrets, LDAP objects and SYSVOL. A restored authority replaces its source; it is not a second independent writable copy.
- **Provider Identity Alias** — A verified one-to-one mapping from a provider's current immutable identifier to ERPX's existing Provider Identity, used to preserve links and audit history during a directory backend migration.
- **Project** — A creator-owned collection of Company work with explicitly managed staff membership.
- **Staff Task** — A piece of operational or Project work with one accountable assignee and optional due date, supporting attachments, and review. Ordinary tasks may be completed by their assignee; controlled tasks retain the approvals required by their policy.
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
- **Calculation Policy** — An approved, effective-dated set of rules used by a deterministic Finance calculation. Changing a policy creates a new version rather than changing prior results.
- **Calculation Run** — A reproducible Finance result pinned to its input, source hash, policy version, currency scale and human decisions. A Run is evidence for a later accounting, filing or publication action; approval alone does not post it.
- **Statutory Package** — A versioned set of tax-return figures, export data and evidence prepared for a jurisdiction and filing period. Exporting a package is distinct from filing or paying it.
- **Bank Match Group** — A reviewed allocation connecting one or more bank movements to one or more accounting targets. It retains its allocation evidence when reversed.
- **Close Check** — A dated, source-versioned result that either passes, warns or blocks an Accounting Period close. A later source change requires a new Check.
- **Manufacturing Cost Movement** — An immutable quantity-and-value event that moves actual cost among raw material, work in progress, finished goods, abnormal loss and cost of sales.
