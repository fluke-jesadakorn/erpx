# ADR-017: Universal record collaboration and human-approved AI

> **Retained architectural decision.** The delivery phases, Rust APIs and verification below describe the 2026-08 implementation. Current adapter coverage and executable owner workflows are in [the catalogue](../contracts/next/catalogue.md); no phase below certifies the present release.
> See [cutover recovery](../../CUTOVER.md) and [ADR-020](ADR-020-nextjs-stack.md).


- Status: Accepted architectural intent; historical Phases 0–6 evidence remains date-scoped.
- Date: 2026-08-21
- Last verified: 2026-08-22

## Context

ERPX began this decision with stable record identities, an Item Directory,
holding-local authorization, collaboration aggregates, version-pinned agent
definitions, and suggestion envelopes, but without one consistent product
surface. This ADR records the architecture used to close those gaps across the
current catalog while preserving owner-context authority and fail-closed
behavior for future records.

The desired product behavior is:

- every current business record can reference every other authorized record;
- one `@` composer can mention people, groups, agents, agent pools, records,
  record versions, and fields;
- every module detail page has field-level references and collaboration;
- a human can assign an approved agent, and an agent can return autofill or
  suggestion output and participate in comments with explicit provenance;
- an agent can never approve, reject, confirm, accept, release, post, pay, or
  otherwise make a policy-classified business decision; and
- access follows Holding, Company, Branch, Department, and Team placement,
  with explicit same-holding sharing and audited global access.

## Decision

The sections below define the accepted architecture. Phases 0-6 are implemented
in the current tree and passed the local repository verifier on 2026-08-22.
Production rollout, canary activation, malware-scanner readiness, and external
provider/directory certification remain operational gates rather than claims
made by this ADR.

### 1. One record identity and one shared collaboration capability

`StableRecordRef` remains the canonical identity for every record and
`RecordVersionRef` pins evidence or a suggestion to a version. Modules must
publish descriptors through the Item Directory and must not create their own
comment, mention, backlink, or generic relation tables.

The Collaboration context owns threads, comments, mentions, attachments,
followers, reactions, and the authoritative typed relation log. Its events
drive a replaceable backlink projection. The Item Directory owns resolution,
search, canonical routes, authorization policy lookup, redaction, freshness,
and serving those projected backlinks. Business contexts continue to own
business fields, lifecycle transitions, and domain-specific relationship
invariants.

Every live record detail surface mounts the same two shared components:

- `RecordReferences`: editable typed relations and backlinks; and
- `RecordActivityRail`: record-, version-, field-, block-, or region-anchored
  comments and activity.

### 2. `@` is a typed token, not an identifier embedded in plain text

The composer displays one `@` entry point. Autocomplete resolves authorized
targets into an immutable token AST:

```text
@{kind}:{stable-id-or-ref}:{optional-version-or-stable-field-id}
```

Supported token kinds are `user`, `group`, `agent`, `agent-pool`, `record`,
`record-version`, and `field`. Labels are snapshots for display only; IDs and
record refs carry identity. Persisted comments keep structured tokens separate
from rendered text so a rename cannot retarget a mention. Copy, edit, search,
notification, redaction, and audit paths round-trip the same typed tokens.
Field tokens carry a stable field ID and schema version; a mutable display path
is never the field identity.

Record search includes at least these canonical types:

| Product concept | Canonical record type |
| --- | --- |
| User | `identity.principal` |
| Contract | `legal.contract` |
| Purchase requisition | `procurement.requisition` |
| Purchase order | `procurement.purchase-order` |
| Goods receipt | `procurement.receipt` |
| Customer invoice | `finance.invoice` |
| Vendor invoice | `finance.vendor-invoice` (new) |
| Payment | `finance.payment` |

Goods receipts and financial receipts must have distinct type labels in the
composer; the UI must never infer meaning from the word “receipt”.

### 3. Relations are typed and direction-aware

A universal relation contains source record/version/field, target
record/version/field, relation kind, creator attribution, created time, and an
optional owner-context evidence reference. Shared relation kinds include
`relates-to`, `depends-on`, `derived-from`, `evidence-for`, `implements`,
`supersedes`, `blocks`, and `custom`.

The shared service validates identity, visibility, same-holding policy, and
backlink publication. The owner business context validates semantic relations.
For example, Procurement decides whether a PO may derive from a PR; Finance
decides whether a vendor invoice may claim a PO or receipt as matching
evidence. A collaboration relation never performs a business transition.

### 4. Agents propose; humans decide

Agents execute with the intersection of the invoking human's effective
authority, the explicit assignment scope, pinned agent-version capabilities,
record policy, current hierarchy, and budget. Provider/model provenance is
recorded. An agent receives only fields and records that intersection permits.
It may:

- return a field patch with confidence and evidence;
- propose a typed record relation;
- draft a comment or reply as an agent-attributed author; and
- request clarification from its assigning human.

It may not directly mutate an owner aggregate, publish a human-authored
comment, expand its scope, grant access, or perform a human-only decision. An
agent-authored result enters `proposed` state. A human may edit, apply, publish,
or discard it. Apply uses optimistic version and source-digest checks and
invokes the owner context's normal command.

Every policy-classified decision command accepts authenticated human evidence,
not a principal string. The evidence must preserve actor kind, session, exact
authority scope, decision reference, and decision time. Authentication and
role/MFA/separation-of-duties checks remain at the command boundary; the owner
domain independently rejects non-human actors. Represented users, agents,
services, workers, groups, and pools cannot satisfy the human decision guard.

Human-only transitions include at minimum approvals and rejections, access
grants, suggestion apply/publish, legal approval, PR approval, PO release,
receipt acceptance, invoice posting, payment release, payroll posting, refund
approval, and any equivalent future-context transition. Each bounded context
must classify new transitions explicitly; omission fails closed.

### 5. Hierarchy and holding access are placement-aware

Authorization uses these levels:

```text
Platform
└── Holding
    └── Company
        ├── Headquarters
        ├── Department
        │   └── Team
        └── Branch
            └── Department
                └── Team
```

Department and Team records use an exact node-root `Subtree` scope. UUID
comparison alone cannot establish ancestry. Organization placement or an
authoritative hierarchy resolver must prove containment. A decision must
re-resolve the current placement within its atomic commit, or pin and verify an
authorization-context/hierarchy revision; the scope in `HumanDecisionProof`
alone is not lineage evidence. Routes may use the holding ID to choose a
physical database, but routing is never authorization.

The access rules are:

- same-node and ancestor access follows the authoritative placement lineage;
- sibling departments or branches are denied by default;
- same-holding cross-branch discovery, mention, reference, and open-record
  access require a bilaterally approved Branch-to-Branch agreement whose grant
  may be directional or mutual; exact Department/Team sharing remains disabled
  until it is placement-aware;
- cross-holding reads are denied by holding-local services and use only the
  audited global gateway defined by ADR-005; and
- comments, autocomplete, backlinks, notifications, and agent context apply
  record authorization before returning labels or existence information.

### 6. Procurement-to-payment is an explicit process, not loose links

The target source chain is:

```text
Contract / budget
       ↓
Purchase requisition → Purchase order → Goods receipt
                              ↘            ↙
                              Vendor invoice → Payment
```

Required owner-context invariants include PR lines and department/budget,
PO source requisition and optional contract, receipt PO-line quantities,
vendor-invoice supplier/PO/receipt evidence, tolerance-aware three-way match,
and payment release against a posted payable. The process manager coordinates
facts and retries but never owns these records or bypasses owner approvals.
The existing customer invoice remains Accounts Receivable and is not reused as
a vendor invoice.

## Delivery plan

### Phase 0 — safety and compatibility foundation

- Preserve authenticated principal kind through server command boundaries.
- Introduce the shared human-decision domain guard and migrate decision paths.
- Give Collaboration confirmation a distinct permission and human-only guard.
- Represent Department/Team as exact node-root scopes in Organization.
- Add placement-aware command/query containment and safe reparent behavior.
- Add additive `Subtree` support to the reference wire contract.
- Read old Organization snapshots, receipts, and projections through bounded
  legacy upcasters; emit a new snapshot version and rebuild projections.
- Add a registration/CI guard requiring every currently reachable decision
  ingress, and every newly registered ingress, to declare its human-only
  policy explicitly.
- Stage Organization deployment reader-first: dual-version readers, a
  capability-gated v2 producer, mixed-version verification, projection rebuild,
  and atomic generation activation must precede default v2 emission.

Phase 0 is code-complete in the current tree. Every registered
decision-shaped ingress is classified and is backed by owner-domain proof or
an explicit owner policy; the registry fails closed for a new unclassified
ingress. Proof construction precedes receipt lookup and mutation, binds the
authenticated human session, action, command/target, expected version, scope,
and trusted time, and is validated again by the owner domain. Identity token
issuance preserves the authoritative principal kind. A UI-only check is never
sufficient.

The implemented Phase 0 safety foundation includes:

- persisted authenticated realm/principal kind on principals and sessions;
  password, passkey, directory, recovery, refresh, active-role selection, and
  access-token authorization preserve and cross-check the same authority;
  legacy sessions whose resolved authority changes are revoked before session
  authority is backfilled, so their refresh bearers must reauthenticate;
- missing realm/kind claims no longer deserialize as a human, and server actor
  construction preserves Agent and Service rather than normalizing them;
- directory identity replacement is a propose/decide flow with distinct
  authenticated principals and sessions, exact proposal/version/scope proof,
  and atomic immutable decision evidence;
- Collaboration action-card confirmation binds the authenticated actor,
  session, scope, command reference, and time and persists that evidence;
- owner-domain human guards for agent-version approval, Operations approval,
  Organization sharing, Data Plane configuration/root decisions, Finance
  period close/reopen, journal posting, and customer-invoice posting, Legal
  contract approval, Procurement PO release plus PR/receipt decision states,
  POS refund review, Sales quote/order decisions, Support session approval,
  and People & Payroll leave/expense/payroll decisions, plus Projects
  dependency and handoff decisions; completed paths reject non-human actors
  before receipt replay and bind proof references to the exact command and/or
  target action;
- exact Department/Team scope, placement-aware reads/writes, safe reparent,
  bounded legacy upcasting, and a disposable-PostgreSQL acceptance harness;
  and
- a closed RPC registry that discovers decision-shaped and `Transition...`
  ingresses and requires each one to declare owner policy, transport guard, or
  domain proof; the former migration placeholders have been closed; and
- a mandatory approved-old protobuf descriptor comparison against the retained,
  explicitly hashed baseline, so compatibility cannot be silently skipped.

Production activation still requires draining authority-unaware old readers
and writers, executing the reader-before-writer Organization rollout,
rebuilding and atomically activating the projection generation, and retaining
exact-node sharing as disabled wherever the owner does not consume authoritative
Department/Team containment. Those are deployment controls; they are not open
Phase 0 implementation gaps.

### Phase 1 — hierarchy rollout and per-context access gates

- Add one authoritative, cached/batched organization-lineage resolver.
- Add normalized node ownership/indexes to each write and read model that
  supports Department/Team isolation; never store a node UUID in a company
  column as a compatibility shortcut.
- Deploy readers before producers, dual-read bounded legacy forms, rebuild an
  atomic projection generation, then enable exact-node writes by capability.
- Add same-holding sharing tests and audited global-access tests per module.
- Keep references, comments, autocomplete, notifications, and agent context
  feature-gated for a context until its complete access matrix passes.

Phase 1 is code-complete for the accepted policy boundary. Its implementation
includes:

- a bounded, transport-neutral `OrganizationLineageWitness` whose distinct
  `organization.lineage_changed` fact carries the current node kind, status,
  parent, ordered lineage, and aggregate version;
- an Organization SQL unit-of-work publisher plus one-query write-side
  resolver that can share-lock the target and every declared ancestor inside
  the caller's atomic transaction;
- an owner-bound directory publication boundary: shared outbox records may
  carry only descriptor drafts whose context, aggregate identity, aggregate
  version, and committed position match the envelope. The generic final-fact
  event remains reserved and is excluded from incremental and rebuild source
  inventories; worker-derived grant/revocation facts are applied only inside
  the trusted reconciliation path;
- a holding-local, rebuildable lineage cache included with IAM, descriptor,
  and grant facts in the Item Directory source fence. Rebuilds also consume
  strictly validated Organization snapshot v1/v2 events, so existing nodes
  are backfilled and a drained-but-still-replaying legacy writer cannot leave
  a positive stale lineage merely because it predates the dedicated fact;
  this security source reads every committed matching outbox sequence and is
  independent of publisher `available_at` retry scheduling, preventing a
  delayed lower sequence from being skipped by the projection cursor. Forward
  control-write and holding-write migrations also move sequence allocation
  into a `BEFORE INSERT` trigger under a transaction-scoped advisory lock,
  reject caller-supplied sequences, and therefore prevent a later sequence
  from committing while an earlier sequence can still commit. The holding
  lock is partitioned by the transaction-bound holding; the control lock is
  database-wide. Two-transaction PostgreSQL acceptance tests prove both
  commit-order contracts;
- grant reconciliation that uses a bounded request-local lineage map and
  rejects missing, inactive, wrong-kind, malformed, sibling, or cross-holding
  evidence; and
- request-side Item Directory and Collaboration filtering that evaluates
  projection freshness, the exact effective grant, policy/version/source
  binding, current lineage, principal, scope, and audience inside the same SQL
  statement snapshot as descriptor/thread payload or authorized count. Every
  participating descriptor, grant, and relation is also upper-bounded by that
  snapshot's `applied_position`, so facts beyond the observed checkpoint stay
  invisible until the checkpoint advances.
  Resolve, search, and backlinks therefore cannot return a replacement
  projection version authorized only by an earlier transaction snapshot, and
  hidden or revoked search matches cannot influence FTS results, pagination,
  or counts; and
- a fail-closed command capability: Collaboration write authorization carries
  the exact visibility scope proven by the same lineage-aware read policy,
  while repositories compare that capability to persisted policy rather than
  reinterpreting a node UUID.

The implementation deliberately keeps reverse `published_to_descendants`
access from a Department/Team session fail-closed. Effective grants do not yet
retain the exact deciding assignment scope and relation, so accepting
containment in both directions would make an unrelated wider assignment
indistinguishable from the decision that produced the grant. Exact-node sharing
also remains disabled; the existing bilaterally approved sharing contract is
Branch-to-Branch only.
Every current record family declares an explicit access policy and unsupported
exact-subtree paths remain denied rather than falling back to kernel UUID
comparison. The production gate is a deny-by-default server capability, not
only this document: exact `Subtree` requests are enabled only for owners that
consume the authoritative resolver and are rejected elsewhere. Rollout
requires a completed Item Directory rebuild generation from the
snapshot-plus-lineage source fence before enabling exact-subtree paths; missing
cache rows remain a safe denial, not a legacy Company/Branch fallback.

### Phase 2 — universal references and activity surfaces

- Expose add/revoke relation commands and full relation projections over
  Collaboration transport.
- Add an authorized record/principal/agent autocomplete service.
- Connect a single Collaboration gateway in Console.
- Mount `RecordReferences` and `RecordActivityRail` on every current detail
  route generated from the live Item Directory catalog.
- Add contract tests proving authorization-before-resolution and redaction.

Status: implemented for all 93 current catalog record families. The shared
`RecordDetailHost` keeps owner content, `RecordReferences`, and the authorized
activity rail independently available; owner-detail absence affects only the
owner slot. Holding-local paths use RLS and the Platform path uses the audited
global gateway. Relations, typed thread V2, comments, reactions, following,
and attachment lifecycle/download reauthorization are wired through Web and
Native collaboration gateways. Future catalog records remain unavailable and
do not mount collaboration. Attachments stay feature-disabled in production
until a malware scanner passes readiness.

### Phase 3 — typed `@` composer and agent assignment bridge

- Implement typed token parsing, rendering, editing, copy/paste, and stale
  label refresh.
- Add an explicit human-confirmed Assign action for `@agent`/`@agent-pool`;
  parsing, editing, or replaying a comment never triggers an assignment.
- Persist invocation, cost, source refs/digests, output, and agent-authored
  comment provenance.
- Require a human review state before publication or owner-command handoff.

Status: implemented. Typed collaboration tokens round-trip separately from
rendered text, authorized autocomplete fails closed, assignment requires an
explicit confirmed human action, and invocation/output/comment provenance is
pinned. Agent output remains proposed until human review; token parsing or
comment replay cannot trigger assignment.

### Phase 4 — universal autofill and suggestions

- Publish owner-defined editable field schemas and patch validation contracts.
- Generalize suggestion envelopes from the existing Sales/Finance capabilities
  to every current record family that declares editable fields.
- Show field-level diff, confidence, evidence, stale-state conflicts, and the
  final human editor/approver in the activity rail.
- Keep high-risk or unsupported fields unavailable instead of falling back to
  untyped JSON mutation.

Status: implemented. Every current family has an explicit editable-field
schema outcome; supported fields use owner-typed validation, version/source
digest checks, durable suggestion projections, human review, and owner command
application. Unsupported fields remain unavailable rather than accepting
opaque JSON.

### Phase 5 — PR to payment vertical slice

- Complete requisition and goods-receipt aggregates, RPCs, projections, and
  Console routes.
- Add PO source refs and contract refs.
- Add Finance-owned vendor invoice and three-way matching.
- Orchestrate accepted receipt to inventory movement and posted payable to
  payment, with human approval at every classified decision.
- Prove end-to-end backlinks, comments, `@` mentions, agent suggestion review,
  audit, idempotency, and read-your-writes behavior.

Status: implemented as a durable owner-separated workflow:
`procurement.requisition` -> `procurement.purchase-order` ->
`procurement.receipt` -> `finance.vendor-invoice` -> `finance.payment`.
Procurement and Finance own their aggregates, evidence, outbox facts,
projections, query freshness, and human-only commands. The Console owner slots
are capability-gated from current, exact-version server responses and remain
inside the shared collaboration host. `finance.invoice` remains the distinct
customer-invoice/Accounts Receivable family.

### Phase 6 — catalog completion and enforcement

- Gate every live catalog record family on descriptor publication, references,
  activity rail, token autocomplete, agent eligibility, and decision policy.
- Add future catalog families only when their owner transport and publisher are
  live; a catalog entry alone is not product completion.
- Make CI fail when a new record detail route omits the shared surfaces or a
  new record family lacks its declared access matrix. Human-only decision
  classification is already enforced from Phase 0.

Status: implemented with exact-set registration for all 93 current families.
Completion composes the actual V2 publisher, owner command/query transport,
access matrix, collaboration policy, editable-field policy, token eligibility,
explicit Agent eligibility, decision classification, canonical route, and
shared-shell markers. CI architecture checks fail closed for omissions and
future-family lookup returns unavailable until the full registration exists.

## Operational rollout status

Local implementation and verification are complete. This decision does not
claim a production rollout, customer activation, production data migration,
external directory/provider certification, malware-scanner readiness, or a
completed canary. Deployment must still follow reader-first protobuf and
schema rollout, publisher/worker activation, atomic Item Directory generation
switching, context canaries, attachment-scanner readiness, and monitored
batch activation. The retained evidence is recorded in
`audits/ADR-017-PHASES-0-6-2026-08-22/README.md`.

## Acceptance criteria

A record family is complete only when all applicable checks pass:

1. It publishes a resolvable, authorized `StableRecordRef` and canonical route.
2. Authorized users can add/revoke typed relations and see redacted backlinks.
3. Record and field comments, attachments, mentions, notifications, and audit
   round-trip through transport and Console.
4. A human can assign an approved agent; its output has pinned provenance and
   cannot mutate the record before human review.
5. Agent/service/worker tokens are rejected by every human-only transition in
   transport, application, and owner-domain tests.
6. Holding, company, branch, department, team, sibling, shared-branch, and
   cross-holding access matrices pass for reads, writes, search, comments,
   backlinks, notifications, and agent context.
7. Snapshot replay, idempotency replay, projection rebuild, old/new protobuf
   compatibility, and rolling deployment tests pass.

## Consequences

ERPX gains a consistent collaboration model without duplicating infrastructure
inside each bounded context. Owner contexts retain their invariants and agents
remain useful without becoming decision authorities. The tradeoff is that
Department/Team authorization and universal AI cannot be enabled by UI changes
alone: identity issuance, every decision ingress, persistence indexes,
transport, projections, and module detail surfaces must migrate in controlled
phases.

Commit-safe outbox ordering deliberately introduces one serialized commit tail
per control-write database and per holding-write holding. Transactions that
emit outbox rows must remain bounded, use the common timeout/retry policy, and
avoid acquiring additional contended locks after the outbox-order lock. The
current multi-aggregate Planning writer pre-acquires that lock before aggregate
row locks; future multi-lock writers require the same ordering review and
two-transaction regression evidence.
