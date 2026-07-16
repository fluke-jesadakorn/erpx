// Pure tests for evaluateTileOptimistic. Run: node app/tests/tileAccess.test.mjs
//
// Persona-neutral tiles: no `requires.roles` allowlist. Access is driven by
// tab+action checks (legacy fallback when no moduleId) and stage refinement.
// The async evaluator (RBAC matrix) is exercised by group-cascade.test.mjs.

const ROLE_PERMISSIONS = {
  staff:               { tabs: ['workbench','pr'], actions: ['submit_expense','submit_pr','view_own_expenses'] },
  accountant:          { tabs: ['workbench','ledger','pr'], actions: ['view_all_expenses','review_expense','settle_payment','semantic_search','view_ledger','view_po','attach_po_payslip'] },
  account_officer:     { tabs: ['workbench','ledger','pr'], actions: ['view_all_expenses','review_expense','approve_expense','reject_expense','semantic_search','view_ledger','approve_pr','reject_pr','view_po','attach_po_payslip'] },
  account_supervisor:  { tabs: ['workbench','ledger','pr'], actions: ['view_all_expenses','review_expense','approve_expense','reject_expense','semantic_search','view_ledger','approve_pr','reject_pr','view_po','attach_po_payslip'] },
  accounting_manager:  { tabs: ['workbench','ledger','pr'], actions: ['view_all_expenses','approve_expense','reject_expense','view_ledger','approve_pr','reject_pr','view_po','approve_po','reject_po','attach_po_payslip'] },
  supervisor:          { tabs: ['workbench','pr'], actions: ['view_all_expenses','approve_expense','reject_expense','approve_pr','reject_pr'] },
  head_of_department:  { tabs: ['workbench','pr','hr'], actions: ['view_all_expenses','approve_expense','reject_expense','approve_pr','reject_pr','view_org_chart','assign_role','set_user_manager','deactivate_user'] },
  manager:             { tabs: ['workbench','ledger'], actions: ['view_all_expenses','approve_expense','reject_expense','view_ledger'] },
  admin:               { tabs: ['workbench','ledger','cockpit','policy','settings'], actions: ['view_all_expenses','approve_expense','reject_expense','view_executive_report','view_ledger','view_policy','edit_policy','ceo_override','manage_ai_providers','manage_ai_models','manage_ai_staff','edit_ai_assignments','view_ai_invocations','view_po','approve_po','reject_po'] },
  cfo:                 { tabs: ['workbench','ledger','cockpit','policy','settings','pr'], actions: ['view_all_expenses','approve_expense','reject_expense','view_executive_report','view_ledger','view_policy','edit_policy','approve_pr','reject_pr','view_ai_invocations','view_po','approve_po','reject_po','attach_po_payslip'] },
  ceo:                 { tabs: ['cockpit'], actions: ['view_executive_report','view_ledger','ceo_override'] },
  it:                  { tabs: ['workbench','pr','ledger','cockpit','policy','settings'], actions: ['view_all_expenses','approve_expense','reject_expense','view_executive_report','view_ledger','view_policy','edit_policy','ceo_override','approve_pr','reject_pr','semantic_search','manage_ai_providers','manage_ai_models','manage_ai_staff','edit_ai_assignments','view_ai_invocations','view_po','approve_po','reject_po','attach_po_payslip'] },
  hr:                  { tabs: ['workbench','hr'], actions: ['view_org_chart','view_user_directory'] },
  hr_manager:          { tabs: ['workbench','hr','ledger','cockpit','policy'], actions: ['view_org_chart','view_user_directory','create_user','update_user','deactivate_user','assign_role','set_user_manager','assign_department_head','view_executive_report','view_ledger'] },
};

const STAGE_TO_ROLE = {
  supervisor_review: 'supervisor',
  head_review: 'head_of_department',
  account_officer_review: 'account_officer',
  account_supervisor_review: 'account_supervisor',
  accounting_review: 'accounting_manager',
  cfo_review: 'cfo',
};

function canAccessTab(role, tab) {
  return ROLE_PERMISSIONS[role]?.tabs.includes(tab) ?? false;
}
function canPerformAction(role, act) {
  return ROLE_PERMISSIONS[role]?.actions.includes(act) ?? false;
}

/**
 * Mirror of evaluateTileOptimistic in components/tileAccess.ts.
 * No role allowlist, no moduleId check (the async evaluator handles RBAC).
 */
function evaluateTileOptimistic(tile, actor) {
  const role = actor?.role_name;
  const requiredTabs = tile.requires.tabs ?? [];
  const requiredActions = tile.requires.actions ?? [];

  // Stage check (CEO/Admin override).
  const stage = tile.requires.approvalStage;
  if (stage && role) {
    const requiredRole = STAGE_TO_ROLE[stage] ?? null;
    if (requiredRole && role !== requiredRole) {
      if (role === 'ceo' || role === 'admin') {
        return { state: 'open', requiredRoles: [requiredRole], requiredTabs, requiredActions, stageOverridable: true };
      }
      return { state: 'stage_locked', requiredRoles: [requiredRole], requiredTabs, requiredActions, stage };
    }
  }

  return { state: 'open', requiredRoles: role ? [role] : [], requiredTabs, requiredActions };
}

/**
 * Mirror of the async evaluator's tab/action fallback (post-RBAC).
 * Used to test the post-RBAC refinement that the async evaluator applies.
 */
function evaluateAfterRbac(tile, actor) {
  const role = actor?.role_name;
  const requiredTabs = tile.requires.tabs ?? [];
  const requiredActions = tile.requires.actions ?? [];

  // 1. Stage check
  const stage = tile.requires.approvalStage;
  if (stage && role) {
    const requiredRole = STAGE_TO_ROLE[stage] ?? null;
    if (requiredRole && role !== requiredRole) {
      if (role === 'ceo' || role === 'admin') {
        return { state: 'open', requiredRoles: [requiredRole], requiredTabs, requiredActions, stageOverridable: true };
      }
      return { state: 'stage_locked', requiredRoles: [requiredRole], requiredTabs, requiredActions, stage };
    }
  }

  // 2. Tab check (legacy fallback)
  if (requiredTabs.length > 0 && (!role || !requiredTabs.some((t) => canAccessTab(role, t)))) {
    return { state: 'locked', requiredRoles: [], requiredTabs, requiredActions, stage };
  }

  // 3. Action check (legacy fallback)
  if (requiredActions.length > 0 && (!role || !requiredActions.some((a) => canPerformAction(role, a)))) {
    return { state: 'locked', requiredRoles: [], requiredTabs, requiredActions, stage };
  }

  return { state: 'open', requiredRoles: role ? [role] : [], requiredTabs, requiredActions, stage };
}

// --- Test cases -------------------------------------------------------------

const cases = [
  // Optimistic: every persona-neutral tile starts as 'open' regardless of role.
  // (Refinement happens in the async RBAC-backed evaluator.)
  { tile: { id: 'feature:submit-expense', requires: { tabs: ['workbench'], actions: ['submit_expense'], moduleId: 'tile-submit-expense' } }, role: 'staff', evaluator: 'opt', expected: 'open' },
  { tile: { id: 'feature:submit-expense', requires: { tabs: ['workbench'], actions: ['submit_expense'], moduleId: 'tile-submit-expense' } }, role: 'cfo',   evaluator: 'opt', expected: 'open' },
  { tile: { id: 'feature:submit-expense', requires: { tabs: ['workbench'], actions: ['submit_expense'], moduleId: 'tile-submit-expense' } }, role: 'hr',    evaluator: 'opt', expected: 'open' },

  // After RBAC allows: tab+action fallback gates by legacy ROLE_PERMISSIONS.
  { tile: { id: 'feature:submit-expense', requires: { tabs: ['workbench'], actions: ['submit_expense'], moduleId: 'tile-submit-expense' } }, role: 'staff',   evaluator: 'post', expected: 'open' },
  { tile: { id: 'feature:submit-expense', requires: { tabs: ['workbench'], actions: ['submit_expense'], moduleId: 'tile-submit-expense' } }, role: 'cfo',     evaluator: 'post', expected: 'locked' }, // cfo has approve_expense but not submit_expense
  { tile: { id: 'feature:submit-expense', requires: { tabs: ['workbench'], actions: ['submit_expense'], moduleId: 'tile-submit-expense' } }, role: 'hr',      evaluator: 'post', expected: 'locked' },

  // Cockpit: cfo/ceo/admin/it
  { tile: { id: 'feature:cockpit', requires: { tabs: ['cockpit'], actions: ['view_executive_report'], moduleId: 'tile-cockpit' } }, role: 'cfo',   evaluator: 'post', expected: 'open' },
  { tile: { id: 'feature:cockpit', requires: { tabs: ['cockpit'], actions: ['view_executive_report'], moduleId: 'tile-cockpit' } }, role: 'admin', evaluator: 'post', expected: 'open' },
  { tile: { id: 'feature:cockpit', requires: { tabs: ['cockpit'], actions: ['view_executive_report'], moduleId: 'tile-cockpit' } }, role: 'ceo',   evaluator: 'post', expected: 'open' },
  { tile: { id: 'feature:cockpit', requires: { tabs: ['cockpit'], actions: ['view_executive_report'], moduleId: 'tile-cockpit' } }, role: 'staff', evaluator: 'post', expected: 'locked' },

  // Stage: CFO stage — admin override allowed via legacy ROLE_PERMISSIONS.
  { tile: { id: 'feature:approve-expense', requires: { tabs: ['workbench'], actions: ['approve_expense'], approvalStage: 'cfo_review', moduleId: 'tile-approve-expense' } }, role: 'cfo',               evaluator: 'opt', expected: 'open' },
  { tile: { id: 'feature:approve-expense', requires: { tabs: ['workbench'], actions: ['approve_expense'], approvalStage: 'cfo_review', moduleId: 'tile-approve-expense' } }, role: 'accounting_manager',evaluator: 'opt', expected: 'stage_locked' },
  { tile: { id: 'feature:approve-expense', requires: { tabs: ['workbench'], actions: ['approve_expense'], approvalStage: 'cfo_review', moduleId: 'tile-approve-expense' } }, role: 'admin',             evaluator: 'opt', expected: 'open' },

  // HR departments: only hr_manager
  { tile: { id: 'feature:departments', requires: { tabs: ['hr'], actions: ['assign_department_head'], moduleId: 'tile-departments' } }, role: 'hr_manager', evaluator: 'post', expected: 'open' },
  { tile: { id: 'feature:departments', requires: { tabs: ['hr'], actions: ['assign_department_head'], moduleId: 'tile-departments' } }, role: 'hr',        evaluator: 'post', expected: 'locked' },

  // Settings: it/admin
  { tile: { id: 'feature:settings', requires: { tabs: ['settings'], actions: ['manage_ai_providers'], moduleId: 'tile-settings' } }, role: 'it',   evaluator: 'post', expected: 'open' },
  { tile: { id: 'feature:settings', requires: { tabs: ['settings'], actions: ['manage_ai_providers'], moduleId: 'tile-settings' } }, role: 'admin',evaluator: 'post', expected: 'open' },
  { tile: { id: 'feature:settings', requires: { tabs: ['settings'], actions: ['manage_ai_providers'], moduleId: 'tile-settings' } }, role: 'cfo',  evaluator: 'post', expected: 'locked' },

  // Anon → locked
  { tile: { id: 'feature:submit-expense', requires: { tabs: ['workbench'], actions: ['submit_expense'], moduleId: 'tile-submit-expense' } }, role: null, evaluator: 'post', expected: 'locked' },

  // Ledger
  { tile: { id: 'feature:ledger', requires: { tabs: ['ledger'], actions: ['view_ledger'], moduleId: 'tile-ledger' } }, role: 'staff',      evaluator: 'post', expected: 'locked' },
  { tile: { id: 'feature:ledger', requires: { tabs: ['ledger'], actions: ['view_ledger'], moduleId: 'tile-ledger' } }, role: 'accountant', evaluator: 'post', expected: 'open' },
  { tile: { id: 'feature:ledger', requires: { tabs: ['ledger'], actions: ['view_ledger'], moduleId: 'tile-ledger' } }, role: 'cfo',        evaluator: 'post', expected: 'open' },
];

let pass = 0, fail = 0;
for (const tc of cases) {
  const fn = tc.evaluator === 'opt' ? evaluateTileOptimistic : evaluateAfterRbac;
  const got = fn(tc.tile, tc.role ? { role_name: tc.role } : null);
  const ok = got.state === tc.expected;
  if (ok) { pass++; console.log(`✓ ${(tc.role || 'anon').padEnd(18)} / ${tc.tile.id.padEnd(28)} [${tc.evaluator}] → ${got.state}`); }
  else {
    fail++;
    console.log(`✗ ${(tc.role || 'anon').padEnd(18)} / ${tc.tile.id.padEnd(28)} [${tc.evaluator}] → ${got.state}, want ${tc.expected}`);
    console.log('  got:', got);
  }
}
console.log(`\n${pass} pass, ${fail} fail`);
process.exit(fail > 0 ? 1 : 0);