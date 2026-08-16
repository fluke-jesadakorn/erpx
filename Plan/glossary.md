# ERPX Glossary

| Term | Definition |
|---|---|
| Active context | The account, role, tenant, organizational scope, and any exceptional support or impersonation authority currently applied to the user. |
| Aggregate | The authoritative business record and invariants changed together by one bounded context. |
| Approval gate | One policy-defined decision checkpoint with explicit eligibility, evidence, outcome, and timing rules. |
| Approval session | A version-pinned instance of an approval policy for a specific source record version. |
| Authoritative write database | The PostgreSQL database containing aggregate state, command receipts, audit evidence, and outbox events for its physical scope. |
| Bounded context | The single business owner of a defined aggregate and lifecycle, exposed through transport-neutral commands and queries. |
| Branch | The lowest canonical organizational scope. Depending on routing mode, a branch uses the holding database pair or its own physical pair. |
| Business workspace | One of the twelve canonical operating areas: Operations, Finance, Sales, Procurement, Inventory, Assets, Maintenance, Quality, Marketing & CRM, HR & Payroll, Point of Sale, or Legal. |
| Command | A request to change authoritative business state. Commands carry scope, actor, idempotency, and version context. |
| Command receipt | Durable evidence that a command identity was accepted or already processed, including its resulting version and correlation-safe outcome. |
| Company | An organizational scope below a holding and above one or more branches. |
| Control plane | The managed authority for identity, authorization, tenant registry, entitlement, platform audit, public presentation, and data-plane coordination. |
| Customer-hosted data plane | A holding-operated environment containing its databases, object storage, directory access, workers, secrets, backup integration, and outbound ERPX agent. |
| Customer 360 | An authorized composed read model of customer identity, organization, commercial, entitlement, onboarding, health, support, access-audit, note, and relationship facts. |
| Data-plane agent | The outbound-only mTLS process that admits signed, versioned, typed ERPX operations to a customer-hosted data plane. |
| Dead letter | An event or work item that exhausted bounded retries and requires governed inspection or replay. |
| Directory connector view | A holding connector or branch-narrowed view that shares one immutable directory authority while applying scoped filters, mappings, and credential profile. |
| Entitlement | Commercial authority for a holding or scope to use a product capability, subject to allocation and billing state. |
| Fresh MFA | A recently completed, backend-verified authentication factor required again for a high-risk action. |
| Holding | The primary customer and physical isolation boundary. A holding owns companies, branches, one directory authority, and its business data routing policy. |
| Idempotency | The property that retrying the same command identity does not create a second business mutation. |
| Integration contract | A versioned fact published by one bounded context for consumption by other contexts without sharing aggregate ownership. |
| Maker-checker | A control requiring a different eligible actor to review or approve an action initiated by the maker. |
| Opaque identifier | A bounded identifier that does not reveal customer names, credentials, topology, or other sensitive meaning. |
| Outbox event | A versioned event committed atomically with an aggregate change for durable worker delivery and projection. |
| Platform | ERPX-managed global scope for product governance and platform operators; it is not standing authority over customer business data. |
| Projection | A replaceable read model built from authoritative events for queries, dashboards, and composed views. |
| Projection freshness | Authoritative metadata indicating whether a read model is current enough for its declared business use. |
| Query | A read request served only from an authorized projection or audited read gateway. |
| Read database | A physically separate, replaceable database containing projections rather than authoritative aggregate state. |
| Read-your-writes | A bounded wait for the read side to acknowledge a command's event; timeout yields not-ready and never falls back to write tables. |
| Represented actor | The customer user whose authority is being used during an approved support or impersonation session, recorded alongside the platform actor. |
| Role assignment | A versioned, expiring grant binding a principal to role, scope, risk, attribution, parent assignment, and validity window. |
| Support capability | A customer-approved, hashed, opaque, time- and scope-bounded grant for a particular support ticket, agent, device/session, represented user, and mode. |
| Tenant | The control-plane registration and routing identity for a customer holding. Business isolation and data routing are resolved through its holding and optional branch routes. |
| Work queue | An authorized projection of assigned or eligible business work, exceptions, and approvals; it is not the authority for the source transaction. |
| Agent assignment | A durable, explicit, version-pinned unit of AI work created by `@agent`, Assign, or Run and governed by scope, budget, lifecycle, and provenance. |
| Agent pool | A governed set of approved agents resolved separately from a human directory group; ordinary routing selects one eligible agent. |
| Anchor record | The first-class lifecycle aggregate root whose data plane, authorization, retention, and legal-hold policy contain a shared thread. |
| Collaboration | The bounded context that owns anchored threads, comment revisions, mentions, reactions, following, moderation, drafts, and relation facts without owning the referenced business record. |
| First-class record | A cataloged lifecycle aggregate root with one owner, opaque reference identity, organizational scope, authorization policy, and canonical domain route. |
| Item Directory | A disposable, rebuildable projection of first-class record descriptors used for authorized resolution, search, and backlinks; it has no business write authority. |
| Mention | An append-only side-effect event created from an authorized `@` token. A human mention notifies; an agent mention creates an assignment. |
| Opaque reference ID | A globally unique, non-enumerable technical identity used by `/reference/item/:id`, distinct from a business document number. |
| Reference relation | A versioned typed edge between two first-class records whose visibility requires authorization at both endpoints. |
| Restricted item | The neutral placeholder shown when a reference exists in conversation structure but the viewer may not discover its metadata or backlink count. |
| Typed action card | A non-executing proposal in a thread that becomes a business command only after confirmation through the owner context. |
