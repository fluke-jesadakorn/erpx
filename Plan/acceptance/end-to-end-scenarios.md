# ERPX End-to-End Acceptance Scenarios

## How to use these scenarios

These scenarios test business outcomes across bounded contexts. They do not
replace each requirement's focused acceptance criteria. A scenario passes only
when authoritative state, projections, authorization, audit, recovery, and the
user-visible outcome agree.

## E2E-01 — Secure platform and holding onboarding

**Actors.** Installer, Platform Root, Holding Owner, independent approver.

**Preconditions.** Approved non-secret database and object profile keys exist;
no prior bootstrap receipt is authoritative.

**Flow.** Run preflight and resumable bootstrap; create the restricted seed;
activate the initial tenant/holding registry; sign in as each initial actor;
publish or inherit the neutral brand; select an active role and scope.

**Pass conditions.** Bootstrap is idempotent and resumable; secrets are shown
once and stored as hashes; runtime starts only from matching receipt and
manifest; Root and Holding Owner authority are distinct; the twelve-workspace
catalog is present; unentitled modules are not actionable; audit contains no
credentials.

## E2E-02 — Branch provisioning and physical isolation

**Actors.** Holding Owner, second customer approver, branch administrator,
provisioning worker.

**Preconditions.** Holding is active and uses an approved branch-pair routing
policy; external credential profiles exist under their approved keys.

**Flow.** Reserve branch and resource identities; approve the high-risk
request; provision and migrate write/read databases; seed selected business
units; verify backup and restore; activate the exact branch route; attempt
sibling and holding access with the branch role.

**Pass conditions.** Partial failure leaves no active route; retry requires
fresh MFA; active branch commands reach only the reserved pair; cross-branch
access is denied; holding consolidation receives branch facts without browser
credential fan-out; route suspension fails closed while preserving recovery
data.

## E2E-03 — Directory onboarding, synchronization, and revocation

**Actors.** Holding Owner, Customer Security Head, directory administrator,
directory user, worker.

**Preconditions.** A Generic LDAP, AD DS, or UCS authority and named credential
profile are available under validated TLS trust.

**Flow.** Preflight the holding connector and branch view; complete full import;
resume the native change feed after restart; map a contained group to an
expiring role; authenticate at the branch; disable the directory account;
process revocation; exercise allowed availability fallback and denied trust
fallback.

**Pass conditions.** Provider immutable IDs prevent username/email merges;
cursor continuity is durable; conflicts require independent replacement
approval; branch mapping cannot escape scope; disable removes access within
five minutes of observation; stale or untrusted feeds fail closed; every
fallback outcome is customer-visible.

## E2E-04 — Customer 360 and customer-approved support

**Actors.** Platform support agent, Holding Owner or Customer Security Head,
customer service user, customer security auditor.

**Preconditions.** Customer and source projections exist; a support ticket is
open.

**Flow.** View the customer list; open an audited detail; request diagnostic
support access; independently approve with fresh MFA; inspect permitted
Customer 360 sections and attachment; perform a represented-user action;
revoke capability and close the ticket.

**Pass conditions.** Section freshness and unavailability are explicit;
restricted data is not returned; capability is ticket-, scope-, device-, and
time-bound; both actors are attributed; revoke or closure denies the next
operation; customer audit shows reads, download, action, and termination.

## E2E-05 — Governed marketing campaign

**Actors.** Marketing operator, marketing approver, Finance approver, content
reviewer, privacy officer, delivery worker.

**Preconditions.** Provider account and route are configured; consent and
suppression authority is available; campaign budget policy is active.

**Flow.** Create content and campaign budget; complete independent approvals;
publish content; build an audience; schedule and execute a journey; withdraw
one participant's consent before delivery; process provider callbacks; erase an
eligible profile; reconcile spend and attribution.

**Pass conditions.** Published versions are pinned; spend cannot exceed
approved budget; send-time consent cancels the withdrawn participant; provider
success is evidenced and callbacks are idempotent; suppression survives
erasure; attribution distinguishes forecast, sales, cash, and recognized
revenue.

## E2E-06 — Lead-to-cash with approval governance

**Actors.** Marketing operator, CRM owner, Sales operator, Sales approver,
Finance approver, fulfillment user, accountant.

**Preconditions.** Customer is active; approval policy and relevant
entitlements are active.

**Flow.** Qualify and hand off a marketing candidate; accept the CRM lead;
progress an opportunity and forecast; create and send a quote; run parallel
Sales/Finance approval gates; accept the pinned quote; confirm and fulfill the
order; activate a revenue arrangement; create/post invoice; record settlement;
recognize revenue from accepted fulfillment.

**Pass conditions.** Every handoff is idempotent and source-linked; changed
quote version invalidates the old approval session; maker-checker and limits
apply; forecast, order, invoice, cash, and revenue remain distinct; Finance
reconciles every amount to source facts; Customer 360 and Operations show fresh
projections without owning the records.

## E2E-07 — Procure-to-pay

**Actors.** Procurement operator, purchasing approver, budget owner, receiving
user, quality inspector, inventory user, accounts-payable user.

**Preconditions.** Supplier, budget, stock item, warehouse, accounts, and
approval policy are active.

**Flow.** Create a purchase order; reserve budget and approve release; receive
stock; perform required inspection; publish quality disposition; update stock;
create the payable; settle it; cancel a remaining eligible line.

**Pass conditions.** Release uses current order and budget versions; denied
approval has no downstream effect; receipt, inspection, movement, payable, and
journal each retain the order reference; duplicates are harmless; quantities
and values reconcile by branch and source position.

## E2E-08 — Sales fulfillment and inventory convergence

**Actors.** Sales operator, warehouse operator, Operations coordinator,
customer service user, accountant.

**Preconditions.** Confirmed sales order and available stock exist.

**Flow.** Reserve stock; partially fulfill order; issue stock; surface a delay
exception; reassign and resolve the exception; complete fulfillment; release
remaining reservation; project Customer 360 and Finance results.

**Pass conditions.** Reservation never exceeds availability; partial and final
states are distinct; Operations does not mutate the source order; duplicate
fulfillment does not duplicate stock or accounting; summary and customer views
identify freshness and branch provenance.

## E2E-09 — Asset, maintenance, and quality recovery

**Actors.** Asset manager, maintenance planner and technician, quality
inspector, Operations coordinator.

**Preconditions.** In-service asset and applicable work/inspection policies
exist.

**Flow.** Mark the asset degraded; open and schedule work; start work and record
downtime; block and resume; complete maintenance; inspect the result; return
the asset to service; resolve the operational exception.

**Pass conditions.** Each context owns its record; downtime is valid and
non-overlapping; return to service requires accepted asset and quality
transitions; replay creates no duplicates; reliability and availability
measures reproduce from source facts.

## E2E-10 — Employee, leave, expense, and payroll

**Actors.** HR operator, employee, manager, payroll operator, payroll approver,
accountant.

**Preconditions.** Organization nodes, accounts, payroll policy, and employee
identity references exist.

**Flow.** Hire an employee; request and approve leave; submit and approve an
expense claim; create, review, approve, and finalize a payroll run; consume
approved expense and payroll facts in Finance; transition employment status.

**Pass conditions.** Person, identity, and employment remain separate; role and
field scope limits hold; approvals are independent; finalized payroll is
immutable; Finance receives minimized, idempotent facts; journals reconcile to
run and claim totals.

## E2E-11 — Point-of-sale sale and refund

**Actors.** Register operator, refund requester, refund approver, inventory
user, accountant.

**Preconditions.** Authorized register, branch, stock, customer reference, and
payment provider-safe method exist.

**Flow.** Open a sale; add lines; hold and resume; record payment; complete;
consume stock and accounting facts; request a partial refund; independently
approve and post it; consume reverse stock and Finance facts.

**Pass conditions.** Completed sale is immutable; prohibited payment data is
absent; refund does not exceed prior unrefunded amount; maker-checker and limits
apply; duplicate facts do not duplicate movement or journal; register, stock,
refund, and Finance totals reconcile.

## E2E-12 — Legal matter, hold, retention, and access

**Actors.** Legal operator, legal approver, authorized reader, privacy officer,
retention worker.

**Preconditions.** Contract parties, protected object namespace, and retention
policy exist.

**Flow.** Create and approve a contract; add immutable document versions; open
a matter and link source records; access a protected document; place a legal
hold; request privacy erasure; attempt archive before and after retention;
release hold through independent authority.

**Pass conditions.** Document digest and versions remain immutable; every read
is audited; matter links do not mutate sources; hold blocks erasure and archive;
retention horizon is enforced; release and final archive retain required audit
and metadata.

## E2E-13 — Projection failure, rebuild, and activation

**Actors.** Business user, platform operator, projector worker, auditor.

**Preconditions.** Authoritative writes and a current read projection exist.

**Flow.** Stop the read target; accept a valid command; query the affected
view; start replacement rebuild; replay duplicates and out-of-order events;
catch up; verify; atomically activate replacement; replay a dead letter.

**Pass conditions.** Write succeeds atomically with receipt and outbox; query
reports unavailable/not-ready and never reads write tables; event ordering and
duplicates are safe; replacement positions, counts, and domain checks converge;
replay is authorized, audited, and non-duplicating.

## E2E-14 — Backup, PITR, and object recovery

**Actors.** Platform recovery operator, independent approver, recovery worker,
auditor.

**Preconditions.** Active tenant or branch route, backup policy, object
inventory, and recovery target exist.

**Flow.** Schedule database/object backup; verify checksums; archive WAL;
restore into isolated targets at an LSN/time; restore required objects; migrate
and start an isolated application read; record RPO/RTO; clean disposable
artifacts.

**Pass conditions.** Scope and checksums match; no credential enters evidence;
restore proves expected business and recovery markers; active route remains
unchanged; cleanup is allow-listed; retained recovery inventory and pass/fail
audit are complete.

## E2E-15 — Clean-generation cutover and rollback

**Actors.** Release operator, business owner, independent approver, platform
worker, auditor.

**Preconditions.** Signed baseline-2.0.0 artifact, newly provisioned
schema-generation-v2 databases, preserved pre-cutover environment, backup,
rehearsed restore, and rollback/forward-recovery plan exist.

**Flow.** Verify normalized schemas, RLS, typed catalogs, 65 services, singular
routes, event namespaces, and empty-state policy; take a release fence without
data import or dual write; atomically switch all ownership to v2; prove legacy
routes/services/events/schema are rejected and new commands succeed; execute
rehearsed rollback or governed forward recovery; re-verify single authority.

**Pass conditions.** All manifests and architecture gates reconcile or block
cutover; no mixed-generation or dual-authority window exists; both switch and
recovery are atomic and audited; pre-cutover databases remain unmodified,
offline from the new runtime, and retained until a separately approved expiry.

## E2E-16 — Customer-hosted data-plane activation

**Actors.** Holding Owner, customer infrastructure operator, customer security
approver, platform operator, data-plane agent.

**Preconditions.** Customer resources, secret provider, outbound network path,
and recovery evidence exist.

**Flow.** Enroll the agent; validate mTLS and signing keys; submit versioned
desired state; preflight databases, objects, directory, workers, backup, and
restore; approve activation; route business commands; request and reject a Root
session; disconnect the agent.

**Pass conditions.** No inbound database or shell access exists; generic SQL
and secrets are unrepresentable; observed state matches desired version;
business behavior and workspace catalog match managed mode; customer rejection
and disconnect deny new brokered work and terminate Root access.

## E2E-17 — Cross-channel, bilingual, accessible work

**Actors.** Business operator using Web and target-native adapters; user with
keyboard and assistive technology; bilingual reviewer.

**Preconditions.** Published platform/holding/business-unit brand profiles and
authorized navigation exist.

**Flow.** Sign in; switch English/Thai; select active role and branch; discover
available and locked modules; complete a shared command; navigate by deep link;
exercise loading, empty, denied, stale, not-ready, conflict, offline, and error
states; repeat shared semantics on target adapters.

**Pass conditions.** Labels, roles, states, receipts, and authority agree across
targets; navigation metadata never authorizes a command; WCAG 2.2 AA behavior
holds; brand cannot override safety controls; credentials never enter URLs or
ordinary storage; unsupported offline work fails safely.

## E2E-18 — Catalog, billing, and entitlement lifecycle

**Actors.** Commercial administrator, billing operator, Holding Owner,
provider adapter, business user.

**Preconditions.** Approved Stripe adapter/version and draft ERPX catalog exist.

**Flow.** Publish catalog revision; create subscription and entitlement;
process signed provider events out of order and with duplicates; allocate
capacity; simulate payment failure and grace; reconcile provider state; recover
payment.

**Pass conditions.** Published catalog is immutable; Stripe owns provider
billing state and ERPX owns entitlements; webhooks are raw-body verified and
idempotent; capacity cannot be exceeded; grace/read-only behavior is explicit;
recovery restores only currently authorized mutations.

## E2E-19 — Platform incident and worker recovery

**Actors.** Platform operator, support lead, Operations coordinator, recovery
operator, customer reader.

**Preconditions.** Health, heartbeat, queue, projection, alert, and runbook
instrumentation exists.

**Flow.** Introduce a worker failure and projection lag; open an incident;
broadcast bounded customer status; observe stale queue and heartbeat; follow
the runbook; replay dead-letter work; verify projections; close incident while
retaining related tickets.

**Pass conditions.** Health and readiness differ correctly; alerts are
redacted and correlated; notifications and replay are idempotent; source
transactions retain their owners; recovery evidence is complete; incident
closure does not silently close customer tickets.

## E2E-20 — Privacy request and customer-visible audit

**Actors.** Data subject or authorized requester, privacy officer, legal
officer, customer security auditor, background worker.

**Preconditions.** Marketing, support, customer, legal, audit, object, and
backup records exist under versioned retention policies.

**Flow.** Submit an authorized erasure request; discover affected stores;
encounter an active legal hold; audit denied and permitted access; release the
hold; erase or anonymize eligible data; retain suppression and required audit;
verify customer access history.

**Pass conditions.** Purpose and field minimization apply; held and legally
required records remain; eligible personal data is removed idempotently;
suppression prevents re-contact; immutable audit retains no prohibited secret;
customer-visible projections converge and show access outcomes.

## Extended enterprise scenarios

The following compact journeys use the same pass rule as E2E-01 through
E2E-20: authoritative state, projections, authorization, audit, recovery, and
visible outcome must agree.

| Scenario | Actors and flow | Pass conditions |
|---|---|---|
| E2E-21 — Enterprise Home and universal work | Authorized users enter `/work`, `/processes`, `/insights`, and `/modules`; filter approvals, tasks, exceptions, incidents, mentions, and agent work; invoke one source action. | Only entitled sources/counts appear; every row declares source/freshness; mutation reaches its owner context idempotently; one unavailable source degrades without fabrication. |
| E2E-22 — Notification delivery | A source creates in-app notice with Email/SMS/Push/Webhook adapters; provider fails, retries, callbacks, and user acknowledges. | Inbox remains authority; delivery deduplicates; preferences and mandatory policy apply; callbacks reconcile; content and secrets are absent from ordinary audit. |
| E2E-23 — Federated search | User searches principals, comments, invoices, contracts, operations, and other first-class records; access is revoked and index is rebuilt. | Authorization occurs before index/match/snippet/facet; hidden counts do not leak; deletion/revocation propagates; rebuilt results converge to source positions. |
| E2E-24 — Reporting and export | User runs, saves, schedules, approves, and downloads an interactive/bulk CSV/XLSX/PDF/internal-API report. | Definition, cutoff, sources, scope, digest, and recipient are manifest-bound; secure link expires; protected rows are absent; file and evidence delete under policy. |
| E2E-25 — Governed import | User uploads versioned file, receives dry-run findings, obtains approval, commits typed commands, corrects failures, and reconciles rows. | File is scanned/digested; dry run is non-mutating; exact approved digest commits under declared atomicity; every row maps to a source receipt or rejection. |
| E2E-26 — Identity governance | HR/directories create a joiner, move scope, certify access, invoke break glass, disable a leaver, rotate service identity, and reassign orphan work. | Realms remain separate; SoD and expiry apply; revocation reaches sessions/work/comments/agents; break glass is reviewed; no work or non-human identity is ownerless. |
| E2E-27 — Electronic signature | Internal approver and external one-time signer sign a pinned document through provider orchestration; callback retries; evidence is exported. | Exact digest, signer order, realm, callbacks, final verification, retention, and audit reconcile; changed/expired document cannot complete; no automatic legal-validity claim appears. |
| E2E-28 — Assistive AI | User asks approved local/external AI to summarize sources and propose a typed action; injection content requests an unauthorized tool. | Retrieval is scoped/cited; injection cannot alter policy; AI cannot mutate; proposed action requires owner-context human/policy approval; uncertainty is visible. |
| E2E-29 — Controlled extension | Admin versions and approves a custom field/form/workflow, indexes/reports it, migrates records, then retires it. | No arbitrary code/SQL runs; core authority and classification hold; core-state mapping remains valid; dependency inventory, rollback, and audit are complete. |
| E2E-30 — Clean-rebuild release | Signed schema-generation-v2 artifact passes internal and production-like gates, then atomically replaces the legacy route/service/event generation; a forced failure exercises rollback. | Artifact digest is constant; architecture, contract, isolation, observability, and recovery gates pass; no mixed-generation traffic or legacy alias is accepted; failure returns to the preserved environment or governed forward recovery with evidence. |
| E2E-31 — Customer exit | Customer requests exit, receives inventory/export, acknowledges, enters write fence/final delta, revokes authority, applies holds, and verifies deletion. | Export manifest reconciles; post-fence writes fail; sessions/providers/agents revoke; held data is sealed; all eligible copies and customer-hosted attestations are evidenced. |
| E2E-32 — Discrete manufacturing | Planner releases make request; manufacturer pins BOM/routing, reserves/issues materials, records labor, genealogy, quality, WIP/cost, and closes. | Inputs/outputs/serials, operations, quality disposition, Inventory, WIP, and Finance facts reconcile; invalid sequence or unavailable material blocks release/close. |
| E2E-33 — Process manufacturing | Operator runs a formula-scaled batch with additions, samples, co-/by-products, scrap, lot genealogy, quality release, and cost close. | Fixed-precision inputs/outputs/yield balance; unapproved addition/release is denied; lineage is bidirectional; unexplained variance blocks close. |
| E2E-34 — MRP/DRP/capacity | Planner pins demand/supply/policy, runs scenarios, reviews shortage/overload/pegging, overrides with reason, and releases selected recommendations. | Replay is deterministic; stale/hidden sources are explicit; draft output has no authority; released buy/make/transfer requests reconcile to owner contexts. |
| E2E-35 — Project delivery and accounting | Team approves services/internal/capital project, baselines WBS, captures time/cost, bills milestones, capitalizes eligible cost, and closes. | Scope/schedule/cost evidence is pinned; source costs reconcile; Projects does not invoice/post assets; all billing/capitalization/close handoffs complete. |
| E2E-36 — E-commerce order and return | Guest/customer browses approved listing, checks out with reservation/payment, receives order/fulfillment, cancels/returns, and observes refund. | Realm and consent remain separate; price/stock are revalidated; one checkout creates one order; payment, shipment, return, refund, and evidence reconcile. |
| E2E-37 — Candidate to talent | Candidate applies/interviews/accepts offer; People accepts hire; employee completes performance/calibration/succession and compensation proposal. | Candidate/workforce realms remain separate; exact offer and selection evidence are pinned; People/Payroll own employment/pay; restricted talent data is not leaked. |
| E2E-38 — Learning record | Owner assigns training, schedules session, records attendance/provider result, approves waiver where needed, and publishes completion to People. | Assignment and evidence versions reconcile; learner cannot self-waive/edit attendance; restricted history is scoped; baseline makes no unsupported LMS/certification claim. |
| E2E-39 — Benefits and payroll | Employee becomes eligible, elects plan/dependents after life event, approval occurs, provider acknowledges, and Payroll posts deductions. | Plan/policy/evidence are pinned; contribution is deterministic; provider and Payroll outcomes reconcile; Benefits does not adjudicate claims or edit payroll. |
| E2E-40 — Field service | Demand creates service order; technician is scheduled/dispatched, records online visit/parts/evidence, customer signs, billing handoff completes, and order closes. | Skill/coverage/Inventory/Asset/customer/billing gates pass; offline mutation is denied; refusal or mismatch creates explicit exception. |
| E2E-41 — Thailand tax and payroll | Thai entity posts invoice/payroll/payment, resolves certified policy, issues e-tax/withholding evidence, submits through adapters, and reconciles responses. | Effective source/rate/form versions are pinned; amounts reconcile; unverified callbacks and unresolved rules are denied; no hard-coded or uncertified claim appears. |
| E2E-42 — Thailand privacy | Verified data subject submits request; system searches sources, evaluates exceptions/holds, responds, deletes eligible copies, and preserves breach/request evidence. | Thailand annex and official-source version are recorded; hidden requester data is protected; unresolved source/hold fails closed; external certification scope is visible. |
| E2E-43 — Banking back-office overlay | Certified banking customer applies classification, IT-risk controls, regulated extract, incident evidence, and integration under BOT overlay. | Only back-office ERP records are in scope; extract and control evidence match certified version; core banking functions are unavailable. |
| E2E-44 — Insurance and healthcare overlays | Certified customer applies insurance or healthcare classification/security/extract/integration controls. | OIC/MOPH source and certification versions are pinned; policy/claims adjudication and clinical operations remain unavailable; changed source triggers recertification. |
| E2E-45 — Human comments and mentions | Participant comments on a business record, mentions a person, edits the comment, follows/mutes, acknowledges, and promotes discussion. | Mention notifies but does not assign; revisions and side-effect events persist; audience never widens; promotion keeps exact source revision and owner command receipt. |
| E2E-46 — Agent mention and retry | Participant mentions an approved agent; worker runs pinned version, waits for input, retries after recoverable failure, and comments as agent. | One durable assignment exists; version/provider/tools remain pinned; human and agent authorship are distinct; retry is idempotent and lineage complete. |
| E2E-47 — Agent delegation | Root agent delegates to approved child agents, attempts excessive depth and a cycle, then completes. | Child scope/budget never exceeds parent; cycle and limits are denied; root/parent/child lineage and costs reconcile; no proactive invocation exists. |
| E2E-48 — Holding/branch mention | Authorized holding user mentions branch person/record; branch user mentions a published holding role/record. | Discovery, mention, notification, and open checks are independent; allowed paths succeed with scoped metadata; content remains in anchor plane. |
| E2E-49 — Sibling denial and shared scope | Branch user searches sibling people/records before and after an explicit shared-scope agreement, then access is revoked. | Before agreement no identity/count leaks; after agreement only allowed items appear; revocation removes thread, preview, search, reference, and agent context. |
| E2E-50 — Cross-domain reference | User inserts `#invoice`, `#contract`, and `#operation`; copies `/reference/item/:id`; singular route/name changes. | Typed opaque refs survive rename/move; authorized resolver redirects to owner route; business IDs are not resolver authority; every open is reauthorized. |
| E2E-51 — Reference redaction | Authorized and unauthorized viewers open the same comment/reference and attempt ID enumeration. | Unauthorized view shows only `Restricted item`; type/title/business ID/count are absent; absent and denied resolver behavior is indistinguishable. |
| E2E-52 — Backlinks | Contracts, invoices, operations, comments, and project items create governed relations; viewers inspect incoming/outgoing links. | Inverses and relation versions reconcile; only mutually authorized links/counts appear; effectful relation requires typed owner command. |
| E2E-53 — Comment lifecycle and hold | Author edits/deletes comment, moderator hides/restores, thread resolves/reopens, record enters legal hold, then purge is requested. | Revisions/tombstone/moderation evidence persist; hold blocks purge; eligible search/cache copies delete; ordinary users cannot see sealed original. |
| E2E-54 — Anchored review | Reviewer comments on exact contract clause and invoice field; documents change and are superseded. | Original digest/locator remains; changed source marks stale/orphaned; thread does not move silently; unauthorized viewers see redaction. |
| E2E-55 — Group mention | User previews and sends team mention; membership changes; user attempts branch-wide broadcast; provider partially fails. | Recipients/group version are snapshotted; new members get no history; broadcast requires capability; fan-out/delivery dedupe and reconcile. |
| E2E-56 — Agent pool | User mentions agent pool, deterministic router selects one agent, then user explicitly compares several agents. | Ordinary pool mention runs one eligible version; candidate/choice are evidenced; Compare previews/charges fan-out budget; each output has provenance. |
| E2E-57 — Rich realtime comment | Web/Desktop/mobile users edit full blocks, code/table/diagram/attachment/reference, receive presence/typing updates, reconnect, and test keyboard/screen reader. | Unsafe blocks quarantine; plain-text fallback exists; stream resumes from cursor; no multi-author edit is claimed; channel accessibility is equivalent. |
| E2E-58 — External portal thread | Customer/candidate/signatory joins explicitly shared thread, mentions approved alias/external-safe agent, while internal users add restricted note. | External directory is not enumerable; internal note never appears; anchor and audience remain scoped; expiry/revocation denies next request. |
| E2E-59 — Typed action card | Human/agent writes approval-like text, proposes action card, authorized owner confirms, and comment is promoted to project issue. | Text/reaction has no authority; exact typed card reaches owner context; retry is idempotent; promoted item and source revision link both ways. |
| E2E-60 — Floating launcher | Internal user opens/closes default-closed launcher across channels, switches agents/sessions/pages, and leaves work running. | Desktop panel and mobile sheet are accessible; preference persists per device; navigation does not mix context silently; closing UI does not cancel assignment. |
| E2E-61 — AI context/provider policy | Launcher auto-attaches authorized page fields including Restricted data to an approved provider; hidden fields and unapproved provider are attempted. | Payload manifest matches current authorization and DPA/residency/classification approval; prohibited fields/provider are blocked; every egress is auditable. |
| E2E-62 — Private AI publication | User explores privately, publishes selected agent summary into anchored thread, then private TTL expires. | Session is private before publication; audience is reauthorized; agent remains author and user is publishing approver; private content deletes separately. |
| E2E-63 — Agent budget and recovery | Concurrent assignments consume tenant/workspace/agent/principal/root budgets; one waits, fails, retries, exceeds budget, and is cancelled. | Reservations and actual use reconcile; exceeded work stops/requests approval; cancellation races are explicit; worker restart resumes safe checkpoints. |
| E2E-64 — Project work graph | Project creates phases/epics/tasks/milestones/deliverables/risks/issues/decisions/changes under governed templates and attempts hierarchy cycle. | Core-state mapping supports common work/reporting; cycle and invalid transition are denied; canonical routes/comments/references and versions remain stable. |
| E2E-65 — Cross-project dependency | Branch projects propose a blocking relation; both owners accept; one revokes and schedule recalculates. | One-sided relation is non-blocking; two-sided accepted versions affect schedule; revoke/cancel reconcile both sides; hidden project details stay redacted. |
| E2E-66 — Project domain handoff | Project proposes invoice, contract, operation, billing, and capitalization actions; owner contexts approve/reject. | Projects never edits source state; typed handoffs preserve idempotency/correlation; accepted and rejected outcomes reconcile into project activity. |
| E2E-67 — Collaboration moderation | External/AI comment contains malware or prohibited content; report, quarantine, redaction, restore, and investigation occur. | Unsafe content never renders; moderator authority and sealed original are controlled; agent/provider provenance remains; evidence supports investigation and retention. |
| E2E-68 — Item Directory recovery | Owner records create/move/merge/archive/delete; descriptor/backlink/search projections lose cursor and rebuild; deletion propagates. | Rebuild never mutates sources; opaque IDs are not reused; gaps stop publication; route/reference/backlink/search/comment/AI copies converge or block verified deletion. |
