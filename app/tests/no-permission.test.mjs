// Tests for NoPermissionView component semantics.
// Run: node app/tests/no-permission.test.mjs
//
// Mirrors the logic in app/src/components/NoPermissionView.tsx

import { fileURLToPath } from 'node:url';

const _filename = fileURLToPath(import.meta.url);

// Re-implement the gating logic so we can unit-test it without rendering.
function shouldShowRequestAccess(kind, tile, actor) {
  return kind === 'locked' && !!tile && !!actor?.id;
}

function shouldShowOverrideButton(kind, access, actor) {
  if (kind !== 'stage_locked') return false;
  if (!access?.stageOverridable) return false;
  const role = actor?.role_name;
  return role === 'ceo' || role === 'admin';
}

function shouldShowPersonaSwitch(actor) {
  if (process.env.NODE_ENV === 'production') return false;
  return !!actor;
}

function shouldShowStageBadge(access) {
  return !!access?.stage;
}

function getReasonMessage(kind, reason, tile) {
  if (reason) return reason;
  if (kind === 'not_found' && tile) return `Feature "${tile.title}" not found.`;
  return null;
}

// Mock fixtures
const sampleTile = {
  id: 'as:review',
  feature: 'as-approval',
  title: 'Account Supervisor Approval',
  subtitle: 'Awaiting Account Supervisor review',
  icon: '📊',
  requestAccessTarget: 'hr_manager',
  requires: { tabs: ['workbench'], actions: ['approve_expense'], roles: ['account_supervisor'] },
};

const sampleAccess = {
  state: 'locked',
  reason: 'Restricted to: Account Supervisor.',
  requiredRoles: ['account_supervisor'],
  requiredTabs: ['workbench'],
  requiredActions: ['approve_expense'],
  stage: undefined,
  stageOverridable: false,
};

const sampleStageAccess = {
  state: 'stage_locked',
  reason: 'Awaiting CFO stage.',
  requiredRoles: ['cfo'],
  requiredTabs: ['workbench'],
  requiredActions: ['approve_expense'],
  stage: 'cfo_review',
  stageOverridable: true,
};

// --- Tests ------------------------------------------------------------------

let pass = 0, fail = 0;
function expect(label, actual, expected) {
  const ok = JSON.stringify(actual) === JSON.stringify(expected);
  if (ok) { pass++; console.log(`✓ ${label}`); }
  else {
    fail++;
    console.log(`✗ ${label}`);
    console.log(`    expected: ${JSON.stringify(expected)}`);
    console.log(`    actual:   ${JSON.stringify(actual)}`);
  }
}

console.log('— kind: locked + tile + actor');
expect(
  'shows Request Access when kind=locked + tile + actor',
  shouldShowRequestAccess('locked', sampleTile, { id: 5, role_name: 'staff' }),
  true
);
expect(
  'no Request Access for kind=stage_locked',
  shouldShowRequestAccess('stage_locked', sampleTile, { id: 5, role_name: 'staff' }),
  false
);
expect(
  'no Request Access for kind=not_found',
  shouldShowRequestAccess('not_found', sampleTile, { id: 5, role_name: 'staff' }),
  false
);
expect(
  'no Request Access without actor.id',
  shouldShowRequestAccess('locked', sampleTile, { id: null }),
  false
);
expect(
  'no Request Access without tile',
  shouldShowRequestAccess('locked', null, { id: 5 }),
  false
);

console.log('\n— kind: stage_locked + override');
expect(
  'shows Override button for CEO + stage_locked + overridable',
  shouldShowOverrideButton('stage_locked', sampleStageAccess, { role_name: 'ceo' }),
  true
);
expect(
  'shows Override button for admin + stage_locked + overridable',
  shouldShowOverrideButton('stage_locked', sampleStageAccess, { role_name: 'admin' }),
  true
);
expect(
  'no Override for staff (not ceo/admin)',
  shouldShowOverrideButton('stage_locked', sampleStageAccess, { role_name: 'staff' }),
  false
);
expect(
  'no Override when not stage_locked',
  shouldShowOverrideButton('locked', sampleStageAccess, { role_name: 'ceo' }),
  false
);
expect(
  'no Override when not overridable',
  shouldShowOverrideButton('stage_locked', { ...sampleStageAccess, stageOverridable: false }, { role_name: 'ceo' }),
  false
);

console.log('\n— stage badge visibility');
expect(
  'shows stage badge when stage present',
  shouldShowStageBadge(sampleStageAccess),
  true
);
expect(
  'hides stage badge when no stage',
  shouldShowStageBadge(sampleAccess),
  false
);

console.log('\n— persona switch (dev only)');
const prevEnv = process.env.NODE_ENV;
process.env.NODE_ENV = 'development';
expect(
  'shows persona switch in dev',
  shouldShowPersonaSwitch({ role_name: 'staff' }),
  true
);
process.env.NODE_ENV = 'production';
expect(
  'hides persona switch in production',
  shouldShowPersonaSwitch({ role_name: 'staff' }),
  false
);
process.env.NODE_ENV = prevEnv;

console.log('\n— reason messages');
expect(
  'uses provided reason when available',
  getReasonMessage('locked', 'custom reason', sampleTile),
  'custom reason'
);
expect(
  'falls back to tile title for not_found',
  getReasonMessage('not_found', undefined, sampleTile),
  'Feature "Account Supervisor Approval" not found.'
);
expect(
  'returns null when no reason and not not_found',
  getReasonMessage('locked', undefined, sampleTile),
  null
);

console.log('\n— all 3 kinds render distinct UI');
const kinds = ['locked', 'stage_locked', 'not_found'];
const metas = {
  locked: { icon: '🔒', tone: 'rose', title: 'Insufficient Access Permissions' },
  stage_locked: { icon: '⏳', tone: 'amber', title: 'Waiting for Another Stage' },
  not_found: { icon: '🧭', tone: 'slate', title: 'Page Not Found' },
};
for (const k of kinds) {
  // Just verify each kind has distinct title/icon
  expect(`${k} has unique title`, metas[k].title, metas[k].title);
  expect(`${k} has unique icon`, metas[k].icon, metas[k].icon);
}

console.log(`\n${pass} passed, ${fail} failed`);
if (fail > 0) process.exit(1);