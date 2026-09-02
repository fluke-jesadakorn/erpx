# ERPX Requirements Traceability Matrix

This matrix covers every normative leaf through inclusive ID ranges. Focused
acceptance paths and sources remain in each specification. Verification method,
evidence ID, and sign-off are derived by the [verification catalog](verification-catalog.md).

| Requirement leaves | Specification | Primary authority | Acceptance scenarios |
|---|---|---|---|
| `REQ-FND-001..006` | [Foundation](../01-product-foundation.md) | ADR-001, ADR-002, ADR-003, ADR-011, ADR-014 | E2E-01, E2E-02, E2E-15, E2E-16 |
| `REQ-IAM-001..019` | [Access](../02-stakeholders-and-access.md) | ADR-004, ADR-006, ADR-013, FSD-002, FSD-005 | E2E-01, E2E-03, E2E-26, E2E-46 |
| `REQ-UX-001..008` | [Shared experience](../03-shared-policies-and-user-experience.md) | ADR-010, FSD-002 | E2E-17, E2E-57, E2E-60 |
| `REQ-POL-001..004` | [Shared policy](../03-shared-policies-and-user-experience.md) | ADR-002, contract policy | E2E-06, E2E-59 |
| `REQ-APR-001..008` | [Approvals](../04-approval-governance.md) | Approval design, ADR-004 | E2E-05, E2E-06, E2E-27 |
| `REQ-WRK-001..012` | [Enterprise work](../05-enterprise-work-and-shared-services.md) | FSD-002, FSD-005, FSD-006 | E2E-21, E2E-45, E2E-60 |
| `REQ-NTF-001..010` | [Enterprise work](../05-enterprise-work-and-shared-services.md) | FSD-002, FSD-005 | E2E-22, E2E-45, E2E-55 |
| `REQ-ESG-001..008` | [Enterprise work](../05-enterprise-work-and-shared-services.md) | FSD-002, FSD-004 | E2E-27 |
| `REQ-PAD-001..006`, `REQ-PAD-090..091` | [Platform governance](../portfolios/01-platform-governance.md) | Platform Administration, FSD-003 | E2E-01, E2E-02, E2E-15, E2E-30 |
| `REQ-ORG-001..004`, `REQ-ORG-090..091` | [Platform governance](../portfolios/01-platform-governance.md) | Organization, ADR-013, FSD-003 | E2E-02, E2E-48, E2E-68 |
| `REQ-COM-001..004`, `REQ-COM-090..091` | [Platform governance](../portfolios/01-platform-governance.md) | Commercial, ADR-007, FSD-003 | E2E-01, E2E-18 |
| `REQ-BRD-001..005`, `REQ-BRD-090..091` | [Platform governance](../portfolios/01-platform-governance.md) | Brand, ADR-012, FSD-003 | E2E-01, E2E-17 |
| `REQ-CUS-001..004`, `REQ-CUS-090..091` | [Customer/support](../portfolios/02-customer-and-support.md) | Customer Authority, FSD-003 | E2E-04, E2E-31, E2E-36 |
| `REQ-C360-001..004`, `REQ-C360-090..091` | [Customer/support](../portfolios/02-customer-and-support.md) | Customer 360, ADR-005, FSD-003 | E2E-04, E2E-16, E2E-68 |
| `REQ-SUP-001..008`, `REQ-SUP-090..091` | [Customer/support](../portfolios/02-customer-and-support.md) | Support, ADR-006, FSD-003 | E2E-04, E2E-19, E2E-20 |
| `REQ-FIN-001..010`, `REQ-FIN-090..091` | [Finance/Sales/POS](../portfolios/03-finance-sales-and-pos.md) | Finance, FSD-003 | E2E-06, E2E-35, E2E-41 |
| `REQ-SAL-001..005`, `REQ-SAL-090..091` | [Finance/Sales/POS](../portfolios/03-finance-sales-and-pos.md) | Sales, FSD-003 | E2E-06, E2E-36 |
| `REQ-POS-001..004`, `REQ-POS-090..091` | [Finance/Sales/POS](../portfolios/03-finance-sales-and-pos.md) | POS, FSD-003 | E2E-11, E2E-36 |
| `REQ-PRC-001..003`, `REQ-PRC-090..091` | [Supply chain/operations](../portfolios/04-supply-chain-assets-and-operations.md) | Procurement, FSD-003 | E2E-07, E2E-34 |
| `REQ-INV-001..004`, `REQ-INV-090..091` | [Supply chain/operations](../portfolios/04-supply-chain-assets-and-operations.md) | Inventory, FSD-003 | E2E-07, E2E-08, E2E-32, E2E-40 |
| `REQ-AST-001..003`, `REQ-AST-090..091` | [Supply chain/operations](../portfolios/04-supply-chain-assets-and-operations.md) | Assets, FSD-003 | E2E-09, E2E-35 |
| `REQ-MNT-001..003`, `REQ-MNT-090..091` | [Supply chain/operations](../portfolios/04-supply-chain-assets-and-operations.md) | Maintenance, FSD-003 | E2E-09, E2E-32 |
| `REQ-QUA-001..003`, `REQ-QUA-090..091` | [Supply chain/operations](../portfolios/04-supply-chain-assets-and-operations.md) | Quality, FSD-003 | E2E-07, E2E-33 |
| `REQ-OPS-001..008`, `REQ-OPS-090..091` | [Supply chain/operations](../portfolios/04-supply-chain-assets-and-operations.md) | Operations, ADR-008, FSD-003 | E2E-19, E2E-21 |
| `REQ-PEO-001..006`, `REQ-PEO-090..091` | [People/legal](../portfolios/05-people-payroll-and-legal.md) | People & Payroll, FSD-003 | E2E-10, E2E-37, E2E-39, E2E-41 |
| `REQ-LEG-001..006`, `REQ-LEG-090..091` | [People/legal](../portfolios/05-people-payroll-and-legal.md) | Legal, ADR-005, FSD-003 | E2E-12, E2E-27, E2E-31, E2E-54 |
| `REQ-MKT-001..012`, `REQ-MKT-090..091` | [Marketing/CRM](../portfolios/06-marketing-and-crm.md) | Marketing, ADR-009, FSD-003 | E2E-05, E2E-06 |
| `REQ-CRM-001..004`, `REQ-CRM-090..091` | [Marketing/CRM](../portfolios/06-marketing-and-crm.md) | CRM, ADR-009, FSD-003 | E2E-06 |
| `REQ-MFG-001..024` | [Manufacturing](../portfolios/07-manufacturing.md) | FSD-003 | E2E-32, E2E-33 |
| `REQ-PLN-001..018` | [Planning](../portfolios/08-supply-and-demand-planning.md) | FSD-003 | E2E-34 |
| `REQ-PRJ-001..026` | [Projects](../portfolios/09-projects.md) | FSD-003, FSD-005, FSD-006 | E2E-35, E2E-64, E2E-65, E2E-66 |
| `REQ-ECM-001..018` | [E-commerce](../portfolios/10-e-commerce.md) | FSD-003 | E2E-36 |
| `REQ-TAL-001..018` | [Talent](../portfolios/11-recruiting-and-talent.md) | FSD-003, FSD-004 | E2E-37 |
| `REQ-LRN-001..012` | [Learning](../portfolios/12-learning.md) | FSD-003 | E2E-38 |
| `REQ-BEN-001..016` | [Benefits](../portfolios/13-benefits.md) | FSD-003 | E2E-39 |
| `REQ-FSV-001..018` | [Field Service](../portfolios/14-field-service.md) | FSD-003 | E2E-40 |
| `REQ-COL-001..033` | [AI/collaboration](../portfolios/15-ai-and-collaboration.md) | FSD-005 | E2E-45, E2E-48, E2E-51, E2E-53, E2E-57, E2E-58, E2E-59, E2E-67 |
| `REQ-AGN-001..030` | [AI/collaboration](../portfolios/15-ai-and-collaboration.md) | FSD-005 | E2E-46, E2E-47, E2E-56, E2E-60, E2E-61, E2E-62, E2E-63 |
| `REQ-DAT-001..009` | [Data/CQRS](../platform/data-integrations-and-cqrs.md) | ADR-002, ADR-014, FSD-006 | E2E-02, E2E-16, E2E-68 |
| `REQ-INT-001..008` | [Data/CQRS](../platform/data-integrations-and-cqrs.md) | Contract policy, provider ADRs, FSD-006 | E2E-03, E2E-18, E2E-30 |
| `REQ-MDM-001..010` | [Master/import/report/search](../platform/master-reference-data-import-reporting-and-search.md) | FSD-002 | E2E-25, E2E-50, E2E-68 |
| `REQ-RPT-001..012` | [Master/import/report/search](../platform/master-reference-data-import-reporting-and-search.md) | FSD-002, ADR-005 | E2E-24 |
| `REQ-IMP-001..010` | [Master/import/report/search](../platform/master-reference-data-import-reporting-and-search.md) | FSD-002 | E2E-25 |
| `REQ-SRC-001..010` | [Master/import/report/search](../platform/master-reference-data-import-reporting-and-search.md) | FSD-002, FSD-005, FSD-006 | E2E-23, E2E-50, E2E-68 |
| `REQ-IFC-001..008` | [Interface catalog](../platform/interface-catalog.md) | Contract policy, FSD-001, FSD-006 | E2E-15, E2E-16, E2E-30 |
| `REQ-AI-001..012` | [AI/extensions](../platform/ai-assistance-and-controlled-extensions.md) | FSD-002, FSD-005 | E2E-28, E2E-46, E2E-61 |
| `REQ-XTN-001..008` | [AI/extensions](../platform/ai-assistance-and-controlled-extensions.md) | FSD-002 | E2E-29 |
| `REQ-REF-001..018` | [Reference routing](../platform/reference-routing-and-collaboration-data.md) | FSD-005, FSD-006, ADR-002, ADR-005 | E2E-50, E2E-51, E2E-52, E2E-54, E2E-68 |
| `REQ-SEC-001..013` | [Security/privacy](../platform/security-privacy-and-audit.md) | ADR-004, ADR-006, ADR-013, FSD-005 | E2E-01, E2E-26, E2E-61 |
| `REQ-AUD-001..004` | [Security/privacy](../platform/security-privacy-and-audit.md) | ADR-005, ADR-006 | E2E-04, E2E-20, E2E-67 |
| `REQ-PRI-001..008` | [Security/privacy](../platform/security-privacy-and-audit.md) | ADR-005, ADR-009, FSD-004 | E2E-20, E2E-31, E2E-42 |
| `REQ-SQA-001..010` | [Security/privacy](../platform/security-privacy-and-audit.md) | FSD-002, FSD-004 | E2E-30, E2E-42, E2E-67 |
| `REQ-DPL-001..005` | [Deployment/continuity](../platform/deployment-continuity-and-migration.md) | ADR-011, ADR-014, FSD-006 | E2E-01, E2E-15, E2E-30 |
| `REQ-BCP-001..009` | [Deployment/continuity](../platform/deployment-continuity-and-migration.md) | Recovery runbooks, FSD-002 | E2E-14, E2E-16, E2E-30 |
| `REQ-MIG-001..007` | [Deployment/continuity](../platform/deployment-continuity-and-migration.md) | ADR-003, ADR-013, FSD-006 | E2E-15, E2E-16, E2E-30 |
| `REQ-OBS-001..007` | [Deployment/continuity](../platform/deployment-continuity-and-migration.md) | Runbooks, observability sources, FSD-002 | E2E-14, E2E-16, E2E-30 |
| `REQ-REL-001..010` | [Deployment/continuity](../platform/deployment-continuity-and-migration.md) | FSD-002, FSD-006, contract policy | E2E-15, E2E-30 |
| `REQ-REG-001..008` | [Jurisdiction packs](../platform/jurisdiction-and-sector-packs.md) | FSD-004, official-source register | E2E-41, E2E-42, E2E-43, E2E-44 |
| `REQ-THA-001..014` | [Jurisdiction packs](../platform/jurisdiction-and-sector-packs.md) | FSD-004, Thai official sources | E2E-41, E2E-42, E2E-43, E2E-44 |
| `REQ-PRT-001..010` | [Customer exit](../platform/customer-exit-and-portability.md) | FSD-002, FSD-004, ADR-011 | E2E-31, E2E-68 |

## Coverage rule

An inclusive range expands to every numeric leaf in that range. Multiple
ranges for a prefix represent intentional stable-ID gaps. A requirement is
traceable only when its definition, source, scenario, deterministic evidence
ID, verification method, and sign-off role all resolve. The validator enforces
that the expanded set equals the definition set exactly.
