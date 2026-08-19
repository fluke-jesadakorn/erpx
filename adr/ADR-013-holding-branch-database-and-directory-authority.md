# ADR-013: Holding-plus-branch database isolation and scoped directory authority

- Status: Accepted
- Date: 2026-08-02

## Context

ERPX originally routes a holding to one holding write/read PostgreSQL pair and
administers directory connectors as platform resources. Customers need to add
branches without ERPX Root intervention, isolate each branch physically, and
administer AD/LDAP using Holding Owner authority. Database and LDAP passwords
must remain outside ERPX transport, state, logs, audit, and SQL records.

Giving every branch an unrelated directory connector would split one human
identity into multiple principals and make hostname fallback unsafe. Switching
an existing branch to a new database without an event watermark and write
fence would also permit lost or duplicated writes.

## Decision

### Installation and database profiles

`erpx-bootstrap` owns the first installation. It validates credential-free
PostgreSQL connection data, applies username/password only to in-memory SQLx
connection options, acquires a PostgreSQL advisory lock, creates and migrates
the control and first holding pairs, and records resumable setup steps. Root
and Holding Owner passwords/recovery codes are generated once; only hashes are
persisted.

Control storage contains approved database profile keys and holding bindings,
never DSNs or credentials. Workers resolve the bound credential profile from
environment/secret-provider configuration and verify that its profile key is
the exact profile reserved on the branch route.

### Physical routing

Holdings have one of two explicit modes:

- `holding_pair`: all holding, company, and branch data uses the holding pair.
- `holding_and_branch_pairs`: holding/company data uses the holding pair and an
  exact active branch scope uses that branch's write/read pair.

The control-plane branch route is authoritative. Provisioning reserves holding,
holding, company, branch, profile, database keys, business-unit selections,
and idempotency key before a worker creates anything. The worker retains
partial resources on failure, migrates and verifies both databases, seeds the
branch organization and its selected fixed business-unit configurations, then
atomically activates the route. A failed job is not automatically retried;
only the fresh-MFA retry API can return it to `queued`.

`SaveOrganizationNode` cannot create a new active branch. It remains the
versioned update path for an already provisioned branch and for other
organization nodes.

### Existing-holding migration and rollback

Migration is explicit and per branch. Durable jobs record only physical keys,
counts, digests, high-watermarks, approval evidence, and rollback fences. The
worker sequence is:

1. verify ownership, scope, profile approval, backups, and capacity;
2. provision candidate write/read databases;
3. copy branch aggregates, receipts, outbox history, and projections;
4. fence branch writes and apply the final delta;
5. compare counts, digests, and source/candidate high-watermarks;
6. activate the exact branch route atomically and retain the old copy until
   `rollback_until`.

Rollback takes the same branch write fence, reverse-replays post-cutover events
to the holding route, verifies the resulting watermark/digest, and only then
restores `holding_pair` routing for that branch. Old copies are not dropped by
the migration or rollback path.

### Holding and branch directory views

A holding has at most one active connector. A branch has at most one active
connector view and must reference the holding connector. Preflight proves the
same provider, immutable directory/forest fingerprint, compatible naming
context, server/CA trust, and immutable identity namespace. A branch may only
narrow OU/base DN, filters, mappings, feed settings, limits, and its named
credential profile.

Canonical identities are keyed by directory authority plus provider immutable
ID. Username and email are collision hints only and never identity merge keys.
Verified holding/branch hostnames select the corresponding active view.

Holding fallback is allowed only for availability failures when the branch and
parent fingerprints still match. Invalid credentials, disabled/deleted
accounts, policy denial, TLS/trust errors, or fingerprint drift fail closed.
The login remains branch-bound and effective roles are recalculated from
branch-authorized mappings. Every accepted or rejected fallback is recorded in
the customer-visible directory audit.

### Local account collisions

Imports create bounded conflict records without merging. A Holding Owner uses
fresh MFA to propose replacement; a different Holding Owner or Customer
Security Head uses separate fresh MFA to approve or reject. Approval suspends
the local customer principal, revokes all password, recovery, TOTP, passkey,
device, challenge, and session credentials, and creates a separate
directory-managed principal. Historical attribution stays on the old
principal. A supersession alias provides display traceability and no local
roles transfer. Platform Root and designated break-glass principals are
permanently protected from this workflow.

## Consequences

- Branch failures cannot expose database URLs or secret values; operators see
  bounded failure categories and durable state.
- Holding/company consolidation must consume branch-local outboxes and publish
  replaceable projections to the holding read database.
- Branch suspension fails route resolution immediately but preserves physical
  data for recovery.
- Operators must provision every database and LDAP credential profile outside
  ERPX using its exact approved profile name.
- Physical isolation costs two PostgreSQL databases per active branch and
  requires branch-aware backup/PITR inventory and monitoring.
- Directory fallback favors safety over login availability whenever authority,
  trust, account policy, or scope cannot be proven.

## Rejected alternatives

- A shared database with branch-only RLS does not meet physical isolation.
- Caller-supplied DSNs or bind passwords would cross public/API and audit
  boundaries.
- Independent branch directories would make immutable identity and fallback
  ambiguous.
- Automatic failed-job retry would bypass the required authorization and fresh
  MFA step.
- Moving roles/history from a local principal to an LDAP principal would
  rewrite authorization provenance.
