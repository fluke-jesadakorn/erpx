# ERPX Future-State Requirements Library

## Purpose and boundary

This library defines the desired business and platform behavior of ERPX. It is
a normative target specification, not an implementation-status report,
delivery backlog, priority list, estimate, or release roadmap. The change is
documentation-only: it does not create software APIs, schemas, migrations, or
runtime behavior.

## Controlled baseline

| Field | Value |
|---|---|
| Baseline version | `2.0.0` |
| Source date | `2026-08-15` |
| Git baseline | `2c9bd786afcd0348054d7623461e5b4a074a09ab` |
| Snapshot | Current working tree, including uncommitted product and design changes |
| Primary channel | Web |
| Target | Clean rebuild: 31 bounded contexts, 21 canonical workspaces, and 67 replacement services |
| Requirement count | Reported by `python3 Plan/tools/validate_requirements.py` |
| Review cadence | Event-driven |

All requirements are target-baseline obligations unless they explicitly state
that a deployment mode, provider, jurisdiction pack, or customer choice is
optional.

## Document map

| Area | Documents |
|---|---|
| Product, actors, shared experience, approvals | [Foundation](01-product-foundation.md), [access](02-stakeholders-and-access.md), [shared policy](03-shared-policies-and-user-experience.md), [approvals](04-approval-governance.md), [enterprise work](05-enterprise-work-and-shared-services.md) |
| Requirements governance | [Governance](06-requirements-governance.md), [future-state decisions](decisions/README.md) |
| Current and future business capabilities | [Portfolio specifications](portfolios/) |
| Data, security, deployment, interfaces, AI, references, regulation, and exit | [Platform specifications](platform/) |
| Journeys, evidence, and traceability | [Acceptance scenarios](acceptance/end-to-end-scenarios.md), [verification catalog](acceptance/verification-catalog.md), [traceability matrix](acceptance/traceability-matrix.md) |
| Regulatory authority | [Regulatory source register](references/regulatory-source-register.md) |
| Vocabulary | [Glossary](glossary.md) |

## Non-normative delivery companion

The [implementation delivery plan](../Delivery/README.md) sequences this
baseline into work packages, phase gates, verification, evidence, and release
controls. Delivery status, estimates, and forecasts remain outside this
normative library.

## Requirement record convention

A normative requirement is either a `### REQ-...` section or a requirement
table row whose first cell is a `REQ-...` identifier. The complete requirement
record is composed of the definition, the deterministic verification metadata
in the verification catalog, and the traceability row. Every leaf requirement
therefore has:

- a stable ID, title, and `ERPX shall...` obligation;
- a business rationale;
- measurable success, denial, and failure/recovery acceptance paths;
- an authoritative repository source or accepted FSD;
- at least one acceptance scenario;
- a verification method, unique required evidence ID, and sign-off role; and
- separate links to existing supporting evidence where it exists.

The evidence ID for a requirement is deterministic:
`EVD-<requirement ID without REQ->-01`. For example, `REQ-COL-004` has
`EVD-COL-004-01`. Existing atomic IDs remain valid. When a compound ID is
split, its parent becomes a non-normative capability family and its normative
children receive alphabetic suffixes such as `REQ-FIN-001A`.

## Prefix registry

| Prefix | Area | Prefix | Area |
|---|---|---|---|
| `FND` | Product foundation | `IAM` | Identity and access |
| `UX` | Shared experience | `POL` | Shared command policy |
| `APR` | Approval governance | `PAD` | Platform administration |
| `ORG` | Organization | `COM` | Commercial and entitlement |
| `BRD` | Corporate identity | `CUS` | Customer authority |
| `C360` | Customer 360 | `SUP` | Support and incidents |
| `FIN` | Finance | `SAL` | Sales |
| `POS` | Point of sale | `PRC` | Procurement |
| `INV` | Inventory | `AST` | Assets |
| `MNT` | Maintenance | `QUA` | Quality |
| `OPS` | Operations | `PEO` | People and payroll |
| `LEG` | Legal | `MKT` | Marketing |
| `CRM` | CRM | `DAT` | Data and CQRS |
| `INT` | Integrations | `SEC` | Security |
| `AUD` | Audit | `PRI` | Privacy |
| `DPL` | Deployment | `BCP` | Continuity and recovery |
| `MIG` | Migration | `OBS` | Observability |
| `WRK` | Enterprise work | `NTF` | Notifications |
| `ESG` | Electronic signatures | `MDM` | Master/reference data |
| `RPT` | Reporting and export | `IMP` | Governed import |
| `SRC` | Federated search | `IFC` | Interface governance |
| `AI` | Assistive AI | `XTN` | Controlled extensions |
| `SQA` | Security assurance | `REL` | Release governance |
| `PRT` | Portability and exit | `REG` | Regulatory overlays |
| `THA` | Thailand pack | `MFG` | Manufacturing |
| `PLN` | Supply/demand planning | `PRJ` | Projects |
| `ECM` | E-commerce | `TAL` | Recruiting and talent |
| `LRN` | Learning records | `BEN` | Benefits |
| `FSV` | Field service | `COL` | Collaboration |
| `REF` | Record references | `AGN` | Agent orchestration |

## Bounded-context catalog

| Class | Bounded contexts |
|---|---|
| Current 21 | Identity & Access; Platform Administration; Organization; Commercial; Brand; Customer Authority; Customer 360; Support; Finance; Sales; Point of Sale; Procurement; Inventory; Assets; Maintenance; Quality; Operations; People & Payroll; Legal; Marketing; CRM |
| Enterprise 8 | Manufacturing; Supply & Demand Planning; Projects; E-commerce; Recruiting & Talent; Learning; Benefits; Field Service |
| Collaboration 2 | Collaboration; Agent Orchestration |

The Item Directory is a disposable federated read projection. It is not a
bounded context and does not own business records.

## Canonical workspace catalog

| # | Workspace | Canonical module route | Channel policy |
|---:|---|---|---|
| 1 | Operations | `/operations` | Web, Desktop, iOS, Android |
| 2 | Finance | `/finance` | Web, Desktop, iOS, Android |
| 3 | Sales | `/sales` | Web, Desktop, iOS, Android |
| 4 | Procurement | `/procurement` | Web, Desktop, iOS, Android |
| 5 | Inventory | `/inventory` | Web, Desktop, iOS, Android |
| 6 | Assets | `/asset` | Web, Desktop, iOS, Android |
| 7 | Maintenance | `/maintenance` | Web, Desktop, iOS, Android |
| 8 | Quality | `/quality` | Web, Desktop, iOS, Android |
| 9 | Marketing & CRM | `/marketing` | Web, Desktop, iOS, Android |
| 10 | HR & Payroll | `/people` | Web, Desktop, iOS, Android |
| 11 | Point of Sale | `/pos` | Web, Desktop, iOS, Android |
| 12 | Legal | `/legal` | Web, Desktop, iOS, Android |
| 13 | Manufacturing | `/manufacturing` | Web, Desktop, iOS, Android |
| 14 | Supply & Demand Planning | `/planning` | Web, Desktop, iOS, Android |
| 15 | Projects | `/project` | Web, Desktop, iOS, Android |
| 16 | E-commerce | `/commerce` | Responsive Web only |
| 17 | Recruiting & Talent | `/talent` | Responsive Web only |
| 18 | Learning | `/learning` | Web, Desktop, iOS, Android |
| 19 | Benefits | `/benefits` | Web, Desktop, iOS, Android |
| 20 | Field Service | `/field-service` | Web, Desktop, iOS, Android |
| 21 | AI & Collaboration | `/ai` | Web, Desktop, iOS, Android |

The canonical module route for AI & Collaboration is `/ai`. Comments and the
floating AI launcher remain
embedded in every authorized domain detail page rather than being confined to
that workspace.

Offline behavior is read-only. Draft creation, comments, mentions, reference
mutations, business commands, and AI invocation require an online authority.

## Authority and conflict resolution

1. An accepted decision that explicitly supersedes another decision wins.
2. Later accepted ADRs govern architecture, isolation, security, and
   interoperability; notably [ADR-013](../adr/ADR-013-holding-branch-database-and-directory-authority.md)
   refines [ADR-001](../adr/ADR-001-holding-isolation.md).
3. Accepted FSDs govern future capabilities not yet represented in code.
4. Bounded-context cores and versioned contracts establish current ownership
   and interface constraints.
5. Accepted designs establish target interactions.
6. Migrations, tests, audits, and runbooks are supporting evidence and cannot
   weaken an accepted invariant.
7. A jurisdiction or sector pack is authoritative only within its declared
   scope and requires the named Legal/Regulatory certification.

Historical sources are migration context; they do not restore legacy write
authority or JavaScript/Next.js technology.

## Clean rebuild architecture

[FSD-006](decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) makes
the following combined architecture authoritative for baseline 2.0.0:

- Domain-Driven Design defines bounded-context ownership, aggregates,
  invariants, domain language, and cross-context contracts.
- Clean Architecture makes dependencies point inward toward domain and
  application policy.
- Hexagonal Architecture expresses external needs as consumer-owned ports and
  locates HTTP, gRPC, SQLx, workers, Dioxus, and provider SDKs in adapters.
- CQRS separates normalized authoritative writes from independently
  rebuildable read projections with explicit freshness.

Domain code depends only on the minimal kernel and its own domain types. It
does not import frameworks, serialized contracts, provider SDKs, or another
context core. The route baseline is singular, stable copied links resolve at
`/reference/item/:id`, and 67 replacement services use `IFC-SVC-201..267`.
Legacy `/workspaces`, `/api/bff`, `/references/:id`, `IFC-SVC-001..065`, event
namespaces, and schema generations receive no compatibility aliases at the
atomic hard cutover.

## Change control

- Product Council owns the baseline; Domain Owners approve business content;
  Architecture, Security, Privacy, Operations, and UX approve their controls.
- Reviews are triggered by accepted decisions, contract changes, regulatory
  changes, or scope changes rather than a calendar schedule.
- A change to a requirement must update its acceptance coverage and
  traceability in the same change.
- Retired IDs are recorded and never reused.
- Delivery status, priorities, estimates, and dates remain outside `Plan/`.

## Validation

Run the documentation-only gate:

```bash
python3 Plan/tools/validate_requirements.py
git diff --check
```

Cargo verification is intentionally unnecessary for this library-only change.
