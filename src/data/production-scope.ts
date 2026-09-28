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

export type ReleaseCriterion = typeof releaseCriteria[number]['id'];
export type GateState = 'pending' | 'passed' | 'not_applicable';
