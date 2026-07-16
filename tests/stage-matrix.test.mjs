// Unit tests for stage-to-module mapping (matrix-driven stage gates).
// Verifies the STAGE_TO_MODULE / STAGE_TO_ROLE maps exported by
// lib/rbac/stage.ts are well-formed and complete.

let pass = 0;
let fail = 0;
const assert = (cond, msg) => {
  if (cond) { pass++; }
  else { fail++; console.error('FAIL:', msg); }
};

const STAGE_TO_MODULE = {
  supervisor_review:         'stage-supervisor-review',
  head_review:               'stage-head-review',
  account_officer_review:    'stage-account-officer-review',
  account_supervisor_review: 'stage-account-supervisor-review',
  accounting_review:         'stage-accounting-review',
  cfo_review:                'stage-cfo-review',
  po_pending:                'stage-po-pending',
  po_cfo:                    'stage-po-cfo',
};

const STAGE_TO_ROLE = {
  supervisor_review:         'supervisor',
  head_review:               'head_of_department',
  account_officer_review:    'account_officer',
  account_supervisor_review: 'account_supervisor',
  accounting_review:         'accounting_manager',
  cfo_review:                'cfo',
  po_pending:                'accounting_manager',
  po_cfo:                    'cfo',
};

// 1. Every stage maps to a stage-* module.
const validStages = Object.keys(STAGE_TO_MODULE);
for (const s of validStages) {
  assert(STAGE_TO_MODULE[s]?.startsWith('stage-'), `STAGE_TO_MODULE[${s}] should map to a stage-* module`);
}

// 2. STAGE_TO_ROLE mirrors the legacy 6 expense + 2 PO stages.
assert(STAGE_TO_ROLE.supervisor_review === 'supervisor', 'supervisor_review → supervisor');
assert(STAGE_TO_ROLE.head_review === 'head_of_department', 'head_review → head_of_department');
assert(STAGE_TO_ROLE.account_officer_review === 'account_officer', 'account_officer_review → account_officer');
assert(STAGE_TO_ROLE.account_supervisor_review === 'account_supervisor', 'account_supervisor_review → account_supervisor');
assert(STAGE_TO_ROLE.accounting_review === 'accounting_manager', 'accounting_review → accounting_manager');
assert(STAGE_TO_ROLE.cfo_review === 'cfo', 'cfo_review → cfo');
assert(STAGE_TO_ROLE.po_pending === 'accounting_manager', 'po_pending → accounting_manager');
assert(STAGE_TO_ROLE.po_cfo === 'cfo', 'po_cfo → cfo');

// 3. No legacy 'manager' stage remains.
assert(!('manager_review' in STAGE_TO_ROLE), 'manager_review should be removed (manager role deleted)');

// 4. Every STAGE_TO_ROLE entry has a corresponding module.
for (const [stage, role] of Object.entries(STAGE_TO_ROLE)) {
  assert(STAGE_TO_MODULE[stage], `${stage} (${role}) has a module mapping`);
}

console.log(`Result: ${pass} pass, ${fail} fail`);
process.exit(fail > 0 ? 1 : 0);