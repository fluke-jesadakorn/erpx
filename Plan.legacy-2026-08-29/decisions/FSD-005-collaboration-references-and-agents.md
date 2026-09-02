# FSD-005 — Collaboration, References, and AI Agents

- Status: Accepted
- Owner: Product Council
- Effective baseline: 1.0.0
- Supersedes: none

## Decision

ERPX adds Collaboration and Agent Orchestration as bounded contexts and adds
AI & Collaboration as the twenty-first workspace. Shared threads are anchored
to authorized first-class business records. Human and agent principals can
comment and mention; `@` addresses principals and `#` addresses typed records.
All lifecycle aggregate roots publish opaque descriptors into a disposable Item
Directory, while their owner contexts retain all business authority.

Collaboration owns thread, revision, mention, reaction, follow, moderation,
draft, reference, and realtime-presence facts. Agent Orchestration owns agent
versions, private sessions, assignments, delegation, pools, budgets, and
invocation provenance. IAM owns principals and Platform Administration owns
provider connectors.

## Authority and isolation

Thread visibility never exceeds the anchor record. Content stays in the anchor
data plane; cross-scope inboxes carry opaque metadata only. Holding-to-child and
child-to-published-parent discovery follows authority; sibling sharing is
explicit and cross-tenant access is denied except through an existing
customer-approved support boundary.

Human mentions notify without assigning. Agent mentions create explicit,
version-pinned asynchronous assignments. Agents have no proactive schedule or
event subscriptions, cannot impersonate people, cannot expand authority, and
cannot mutate business state except through a human/policy-approved typed
action card.

## Experience

Internal channels expose one default-closed floating launcher for multiple
context-bound sessions. Page context is attached automatically after current
field authorization. Approved external providers may receive Confidential or
Restricted context only under tenant allow-list, DPA, residency, classification,
and provenance controls. Private sessions use a separate TTL and publish into
shared threads only by explicit user action.
