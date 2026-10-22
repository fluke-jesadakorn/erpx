/** The release cut is independent of the full capability inventory. */
export const productionScope = {
  version: 'thai-hotel-v1-crm-analytics',
  country: 'TH',
  currency: 'THB',
  companies: 1,
  establishments: 1,
  modules: ['organization', 'identity_access', 'people_payroll', 'operations',
    'procurement', 'inventory', 'finance', 'pos', 'pms', 'crm', 'marketing', 'collaboration'],
  deferred: ['ota', 'public_booking', 'offline_pos', 'payment_gateway', 'multi_hotel',
    'executive', 'autonomous_ai', 'marketing_automation', 'manufacturing'],
} as const;

export const releaseCriteria = [
  { id: 'documentation', label: 'Current plan, contracts, links and evidence checksums', owner: 'Engineering', permission: 'organization.manage' },
  { id: 'hotel', label: 'Staff booking through Night Audit, corrections and checkout', owner: 'Hotel manager', permission: 'pms.approve' },
  { id: 'crm_analytics', label: 'Reviewed identities, lead to confirmed Sales or PMS outcome, posted net revenue, repeat business and scoped CRM reports', owner: 'Sales / Hotel manager', permission: 'commerce.approve' },
  { id: 'marketing_analytics', label: 'Form intake, campaign/source evidence, first and last touch, explicit posted costs and scoped Marketing reports', owner: 'Marketing / Finance', permission: 'records.write' },
  { id: 'restaurant', label: 'Kitchen, stock, verified tenders, room charge, refunds and shift close', owner: 'Restaurant manager / Finance', permission: 'pos.approve' },
  { id: 'procurement', label: 'Request, RFQ, PO, partial receipt, invoice match and supplier reconciliation', owner: 'Purchasing / Finance', permission: 'organization.manage' },
  { id: 'hr', label: 'Employment, reporting manager, access, roster, time, leave, OT and payroll handoff', owner: 'HR', permission: 'people.manage' },
  { id: 'thai_payroll', label: 'Independent Thai statutory expected results for every agreed employee group', owner: 'Thai payroll specialist', permission: 'payroll.approve' },
  { id: 'ai', label: 'Human-confirmed purchase drafts and source-linked day-close assistance, including provider failure', owner: 'AI / Source owners', permission: 'organization.manage' },
  { id: 'money_access', label: 'Independent approval, stale/repeated commands, revoked access and cross-company denial', owner: 'Finance / IT', permission: 'finance.approve' },
  { id: 'release', label: 'Lint, typecheck, integration, migration upgrade and production browser acceptance', owner: 'Engineering / QA', permission: 'organization.manage' },
  { id: 'operations', label: 'Off-machine restore (RPO 1h / RTO 4h), rollback and failed-job alerts', owner: 'Operations / IT', permission: 'organization.manage' },
  { id: 'hotel_signoff', label: 'Real hotel configuration and two consecutive reconciled day closes', owner: 'Hotel manager', permission: 'pms.approve' },
  { id: 'payroll_signoff', label: 'HR and Payroll sign-off after one independently reconciled full payroll cycle', owner: 'HR / Payroll', permission: 'payroll.approve' },
  { id: 'finance_signoff', label: 'Finance sign-off on accounting, settlement and external filing controls', owner: 'Finance', permission: 'finance.approve' },
] as const;

/** Legal acceptance is per Company and does not inherit the hotel pilot limits. */
export const legalProductionScope = {
  version: 'legal-th-operations-v1', country: 'TH', modules: ['legal'],
  deferred: ['electronic_signing', 'automatic_destruction', 'government_filing', 'industry_packs'],
} as const;

export const legalReleaseCriteria = [
  { id: 'legal_workflows', label: 'Scoped Legal UAT journeys, independent review and executed copies', owner: 'Legal owner / QA', permission: 'legal.manage' },
  { id: 'legal_content', label: 'Named legal specialist accepts rules, templates, applicability and Company policy', owner: 'Thai legal specialist', permission: 'legal.manage' },
  { id: 'legal_access', label: 'Current assignments, confidential membership and cross-Company denial', owner: 'IT / Legal reviewer', permission: 'organization.manage' },
  { id: 'legal_documents', label: 'Quarantine, scanner, OCR, provenance, private files and legal holds', owner: 'IT / Legal reviewer', permission: 'legal.manage' },
  { id: 'legal_ai', label: 'Company provider policy, citation evaluation, insufficient evidence and provider failure', owner: 'Legal specialist / AI owner', permission: 'legal.manage' },
  { id: 'legal_release', label: 'Lint, typecheck, migration upgrade and isolated production browser acceptance', owner: 'Engineering / QA', permission: 'organization.manage' },
  { id: 'legal_operations', label: 'Jobs, email, off-machine restore (RPO 1h / RTO 4h), rollback and alerts', owner: 'Operations / IT', permission: 'organization.manage' },
] as const;

export type ReleaseScopeVersion = typeof productionScope.version | typeof legalProductionScope.version;
export const allReleaseCriteria = [...releaseCriteria, ...legalReleaseCriteria] as const;
export function criteriaForScope(scope: ReleaseScopeVersion) {
  return scope === legalProductionScope.version ? legalReleaseCriteria : releaseCriteria;
}
export type ReleaseCriterion = typeof allReleaseCriteria[number]['id'];
export type GateState = 'pending' | 'passed' | 'not_applicable';
