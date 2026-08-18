# People, Payroll, and Legal Requirements

## Portfolio boundary

People & Payroll owns employment, leave, employee expenses, and payroll runs.
Legal owns contracts, documents, matters, legal holds, record links, and access
history. Identity accounts, organization structure, payment execution, and the
general ledger remain in their owning contexts.

## People and Payroll

### REQ-PEO-001 — Employment lifecycle

**Requirement.** ERPX shall manage employment through hire and authorized
status transitions while keeping identity and organization references external
to the employment aggregate.

**Rationale.** A person, login account, organization node, and employment are
related but not interchangeable identities.

**Acceptance criteria.** Hiring references valid person and organization
identities; employee ID is stable; transitions are versioned and auditable;
inactive employment denies new employee work under policy without deleting
history.

**Sources.** [People & Payroll context](../../crates/contexts/people-payroll/src/lib.rs),
[People & Payroll API](../../crates/proto/proto/erpx/people_payroll/v2/service.proto).

### REQ-PEO-002 — Scoped employee directory

**Requirement.** ERPX shall provide authorized employee list and detail views
by holding, company, branch, status, and permitted fields.

**Rationale.** Managers and payroll staff need different views of workforce
data.

**Acceptance criteria.** Queries use projections and expose freshness; field
visibility follows purpose and role; cross-branch access requires parent-scope
authority; credentials and unrelated identity-provider data are excluded.

**Sources.** [People & Payroll context](../../crates/contexts/people-payroll/src/lib.rs),
[People & Payroll API](../../crates/proto/proto/erpx/people_payroll/v2/service.proto).

### REQ-PEO-003 — Leave lifecycle

**Requirement.** ERPX shall manage leave requests through creation, review,
approval or rejection, cancellation, and completion according to policy and
manager authority.

**Rationale.** Leave affects staffing and payroll and requires explainable
decisions.

**Acceptance criteria.** Request identifies employee, type, interval, scope,
and version; invalid or overlapping periods are rejected under policy;
maker-checker and manager eligibility apply; approved leave publishes a
versioned workforce fact.

**Sources.** [People & Payroll context](../../crates/contexts/people-payroll/src/lib.rs),
[people-payroll domain](../../crates/contexts/people-payroll/src/domain/mod.rs).

### REQ-PEO-004 — Employee-expense lifecycle

**Requirement.** ERPX shall manage employee expense claims through submission,
review, approval or rejection, posting readiness, and payment handoff.

**Rationale.** Reimbursement requires receipt evidence, policy control, and
separation from payment execution.

**Acceptance criteria.** Claim records employee, scope, currency, lines,
evidence references, and version; limits and maker-checker apply; duplicate
evidence is detected under policy; approved claims emit a Finance fact and do
not directly create payment.

**Sources.** [People & Payroll context](../../crates/contexts/people-payroll/src/lib.rs),
[People & Payroll API](../../crates/proto/proto/erpx/people_payroll/v2/service.proto).

### REQ-PEO-005 — Payroll-run lifecycle

**Requirement.** ERPX shall manage payroll runs through creation, calculation,
review, approval, finalization, and cancellation under exact period and scope.

**Rationale.** Payroll is high-risk, privacy-sensitive, and financially
material.

**Acceptance criteria.** A run identifies period, currency, employee scope,
line inputs, policy version, and totals; finalization requires independent
approval and current source versions; finalized lines are immutable;
cancellation follows explicit pre-finalization rules.

**Sources.** [People & Payroll context](../../crates/contexts/people-payroll/src/lib.rs),
[People & Payroll API](../../crates/proto/proto/erpx/people_payroll/v2/service.proto),
[HR projection](../../apps/worker/src/hr_projection.rs).

### REQ-PEO-006 — Finalized payroll facts

**Requirement.** ERPX shall publish finalized payroll facts for Finance and
Operations without exposing unnecessary employee detail or granting consumers
payroll mutation authority.

**Rationale.** Accounting and planning need payroll outcomes with data
minimization.

**Acceptance criteria.** Facts identify run, scope, currency, accounting
allocations, totals, and source version; sensitive line detail is limited to
authorized consumers; duplicate consumption is idempotent; Finance journals
retain the payroll source reference.

**Sources.** [People & Payroll context](../../crates/contexts/people-payroll/src/lib.rs),
[people-payroll integration](../../crates/contexts/people-payroll/src/integration/mod.rs),
[Finance context](../../crates/contexts/finance/src/lib.rs).

## Legal

### REQ-LEG-001 — Contract lifecycle

**Requirement.** ERPX shall manage legal contracts, parties, terms, approval,
execution state, termination, and archive through explicit transitions.

**Rationale.** Contract authority and retention need an immutable, reviewable
record.

**Acceptance criteria.** Contract identity and version are stable; parties and
scope are validated; terminal transitions require their defined evidence;
archive is prohibited until retention and legal-hold rules permit it.

**Sources.** [Legal context](../../crates/contexts/legal/src/lib.rs),
[Legal API](../../crates/proto/proto/erpx/legal/v2/service.proto).

### REQ-LEG-002 — Versioned legal documents

**Requirement.** ERPX shall store legal documents as immutable versions with
digest-addressed object references, classification, purpose, and access policy.

**Rationale.** Legal evidence must not change behind an existing approval or
audit record.

**Acceptance criteria.** Adding content creates a new version; object digest
and metadata are verified; prior versions remain readable under retention and
permission; arbitrary public URLs and credentials are rejected.

**Sources.** [Legal context](../../crates/contexts/legal/src/lib.rs),
[legal persistence](../../crates/infrastructure/src/legal_persistence.rs),
[Legal API](../../crates/proto/proto/erpx/legal/v2/service.proto).

### REQ-LEG-003 — Legal matter graph

**Requirement.** ERPX shall manage legal matters and versioned links to
approved records without taking ownership of the linked source records.

**Rationale.** A matter needs a traceable case view across contracts,
customers, people, transactions, and documents.

**Acceptance criteria.** Links identify source context, record, relation,
version, and scope; missing or unauthorized targets do not disclose detail;
closing a matter does not mutate linked records; graph projection is
rebuildable.

**Sources.** [Legal context](../../crates/contexts/legal/src/lib.rs),
[legal projection](../../apps/worker/src/legal_projection.rs),
[Legal API](../../crates/proto/proto/erpx/legal/v2/service.proto).

### REQ-LEG-004 — Legal hold

**Requirement.** ERPX shall place and release legal holds through authorized,
reasoned, versioned commands and shall make an active hold override ordinary
deletion or archive eligibility.

**Rationale.** Required evidence must survive normal retention processing.

**Acceptance criteria.** Hold identifies matter, protected records, reason,
issuer, time, and scope; duplicate hold commands are idempotent; release
requires appropriate independent authority; active hold blocks archival and
erasure of covered content.

**Sources.** [Legal context](../../crates/contexts/legal/src/lib.rs),
[legal domain](../../crates/contexts/legal/src/domain/mod.rs),
[legal retention worker](../../apps/worker/src/legal_retention.rs).

### REQ-LEG-005 — Bounded retention and archive

**Requirement.** ERPX shall enforce contract and document retention periods and
shall archive only after the retention threshold has elapsed and no active hold
applies.

**Rationale.** Early deletion and indefinite uncontrolled retention both create
legal risk.

**Acceptance criteria.** Retention deadline is validated against the permitted
horizon; archive before deadline is rejected; retention processing is
idempotent and restartable; audit and minimum required metadata remain after
archive.

**Sources.** [Legal context](../../crates/contexts/legal/src/lib.rs),
[legal retention worker](../../apps/worker/src/legal_retention.rs),
[legal retention tests](../../apps/worker/tests/legal_retention.rs).

### REQ-LEG-006 — Legal access history

**Requirement.** ERPX shall record and expose an authorized immutable history
of legal document and matter access with actor, purpose, scope, and outcome.

**Rationale.** Highly sensitive legal records require evidence of both
successful and denied access.

**Acceptance criteria.** Access is recorded before detail is returned; history
includes platform and represented actors where applicable; denied access does
not expose record content; entries cannot be edited or deleted by the reader.

**Sources.** [Legal context](../../crates/contexts/legal/src/lib.rs),
[Legal API](../../crates/proto/proto/erpx/legal/v2/service.proto),
[audited reads](../../adr/ADR-005-global-read-auditing.md).

## Lifecycle-completion requirements

| ID | Title | Normative requirement | Rationale | Acceptance paths | Source | Scenarios |
|---|---|---|---|---|---|---|
| REQ-PEO-090 | People and payroll lifecycle coverage | ERPX shall govern candidate handoff, worker, employment, assignment, leave, time, expense, payroll, correction, termination, archive, and retention. | Workforce and pay need complete effective-dated states. | Success: valid transition links authority; Denial: unauthorized retroactivity is rejected; Failure: partial payroll remains unfinalized. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-10, E2E-37, E2E-39 |
| REQ-PEO-091 | People and payroll reconciliation | ERPX shall reconcile organization, IAM, time, expense, benefits, talent, learning, payroll provider, bank, tax, and Finance facts. | Employee state and pay span contexts. | Success: finalized run balances accepted facts; Denial: consumer cannot edit employment/payroll; Failure: mismatch blocks finalization. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-10, E2E-39, E2E-41 |
| REQ-LEG-090 | Legal lifecycle coverage | ERPX shall govern document, contract, matter, clause, party, approval, signature, obligation, hold, closure, supersession, and archive. | Legal records need exact versions and retention. | Success: valid state pins digest/authority; Denial: held/signed history cannot be overwritten; Failure: incomplete signature/evidence blocks activation. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-12, E2E-27, E2E-54 |
| REQ-LEG-091 | Legal reconciliation | ERPX shall reconcile parties, obligations, signatures, holds, attachments, projects, customer exit, retention, and audited access. | Legal effects propagate broadly. | Success: consumers acknowledge exact version; Denial: consumer cannot release hold; Failure: mismatch blocks purge/closure. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-12, E2E-31, E2E-54 |
