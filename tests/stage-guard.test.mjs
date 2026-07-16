// Pure tests for stage guard. Mirrors policy/engine.ts + guard.ts requireAction.
// Verifies:
//   - the right role unlocks each stage
//   - CEO/Admin can override out-of-stage approvals
//   - everyone else is blocked

const STAGE_TO_ROLE = {
  supervisor_review: 'supervisor',
  head_review: 'head_of_department',
  account_officer_review: 'account_officer',
  account_supervisor_review: 'account_supervisor',
  accounting_review: 'accounting_manager',
  cfo_review: 'cfo',
};

function checkStage(stage, role) {
  const requiredRole = STAGE_TO_ROLE[stage];
  if (!requiredRole) return { allowed: false, reason: 'unknown stage' };
  if (role === requiredRole) return { allowed: true, override: false };
  if (role === 'ceo' || role === 'admin') return { allowed: true, override: true };
  return { allowed: false, override: false, reason: `requires ${requiredRole}` };
}

let pass = 0, fail = 0;
function check(name, got, want) {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (ok) { pass++; console.log(`✓ ${name} → ${JSON.stringify(got)}`); }
  else    { fail++; console.log(`✗ ${name} got=${JSON.stringify(got)} want=${JSON.stringify(want)}`); }
}

// Each stage accepts its natural role
check('supervisor_review + supervisor',        checkStage('supervisor_review', 'supervisor'),         { allowed: true, override: false });
check('head_review + head_of_department',       checkStage('head_review', 'head_of_department'),     { allowed: true, override: false });
check('account_officer_review + account_officer', checkStage('account_officer_review', 'account_officer'), { allowed: true, override: false });
check('cfo_review + cfo',                       checkStage('cfo_review', 'cfo'),                     { allowed: true, override: false });

// Out-of-stage: blocked for non-override roles
check('cfo_review + accounting_manager',         checkStage('cfo_review', 'accounting_manager'),     { allowed: false, override: false, reason: 'requires cfo' });
check('supervisor_review + accounting_manager', checkStage('supervisor_review', 'accounting_manager'), { allowed: false, override: false, reason: 'requires supervisor' });
check('cfo_review + staff',                     checkStage('cfo_review', 'staff'),                   { allowed: false, override: false, reason: 'requires cfo' });

// CEO/Admin can override out-of-stage approvals (with audit row written)
check('cfo_review + ceo (override)',            checkStage('cfo_review', 'ceo'),                     { allowed: true, override: true });
check('cfo_review + admin (override)',          checkStage('cfo_review', 'admin'),                   { allowed: true, override: true });
check('supervisor_review + ceo (override)',     checkStage('supervisor_review', 'ceo'),              { allowed: true, override: true });
check('head_review + admin (override)',         checkStage('head_review', 'admin'),                  { allowed: true, override: true });

// HR Manager cannot override (no executive privilege)
check('cfo_review + hr_manager',                checkStage('cfo_review', 'hr_manager'),              { allowed: false, override: false, reason: 'requires cfo' });

// IT cannot override (no executive privilege, even though they have many actions)
check('cfo_review + it',                        checkStage('cfo_review', 'it'),                      { allowed: false, override: false, reason: 'requires cfo' });

// Unknown stage
check('unknown stage',                          checkStage('wibble_stage', 'staff'),                 { allowed: false, reason: 'unknown stage' });

console.log(`\n${pass} pass, ${fail} fail`);
process.exit(fail > 0 ? 1 : 0);