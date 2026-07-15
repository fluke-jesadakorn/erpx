// Pure-logic tests for the cross-domain visibility resolver.
//
// We re-implement the small labelForScope / mapEventToDomain functions
// here as mirrors. The real implementations live in:
//   - app/src/lib/rbac/visibility.ts:labelForScope (server-only)
//   - app/src/lib/notifications/recipients.ts:mapEventToDomain
//
// These tests assert the contract: scope resolution, deny wins, team
// label composition, event→domain mapping.

let pass = 0;
let fail = 0;

function eq(actual, expected, msg) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) {
    pass++;
    console.log(`  ✓ ${msg}`);
  } else {
    fail++;
    console.log(`  ✗ ${msg}\n    expected: ${e}\n    actual:   ${a}`);
  }
}

// --- Mirror of mapEventToDomain -------------------------------------------
const DOMAIN_BY_REF = {
  expense: 'expenses', pr: 'pr', po: 'po', slip: 'slips',
  user: 'users', department: 'departments',
  audit: 'audit', ai: 'ai_settings', notification: 'notifications',
};
function mapEventToDomain(type, refType) {
  if (refType && DOMAIN_BY_REF[refType]) return DOMAIN_BY_REF[refType];
  if (type.startsWith('expense.')) return 'expenses';
  if (type.startsWith('pr.')) return 'pr';
  if (type.startsWith('po.')) return 'po';
  if (type.startsWith('slip.')) return 'slips';
  if (type.startsWith('user.')) return 'users';
  if (type.startsWith('audit.')) return 'audit';
  if (type.startsWith('ai.')) return 'ai_settings';
  if (type.startsWith('notification.') || type.startsWith('notif.')) return 'notifications';
  return null;
}

// --- Mirror of labelForScope ----------------------------------------------
function labelForScope(kind, teamIds, teamNamesById, fallbackScope, department) {
  switch (kind) {
    case 'self':       return 'Self only';
    case 'department': return department ? `Dept · ${department}` : 'Department';
    case 'team': {
      if (teamIds.length === 0) return 'Team (no teams assigned)';
      return teamIds.map((id) => teamNamesById[id] ?? id).join(', ');
    }
    case 'all':        return 'All (company-wide)';
    case 'subtree':    return 'Subtree (reportees)';
    case 'deny':       return 'Deny';
    default:           return `Default · ${fallbackScope}`;
  }
}

// --- Mirror of scope resolution (server logic, no DB) ---------------------
// The real resolver prefers rbac.domain_scope over rbac.roles.scope_kind.
// Here we model the lookup + fallback as pure data.
function resolveScope({ roleScopes, roleFallback, domainId, roleId }) {
  if (roleScopes[roleId] && roleScopes[roleId][domainId]) {
    return { kind: roleScopes[roleId][domainId], source: 'domain_scope' };
  }
  return { kind: roleFallback[roleId] ?? 'self', source: 'role_default' };
}

console.log('visibility: scope resolution');

const roleScopes = {
  L4: { expenses: 'all', notifications: 'all' },
  L3: { expenses: 'department' },
  L2A: { expenses: 'self' },
  L2B: {},
};
const roleFallback = { L1: 'self', L2A: 'self', L2B: 'department', L3: 'department', L4: 'all' };

eq(
  resolveScope({ roleScopes, roleFallback, domainId: 'expenses', roleId: 'L4' }),
  { kind: 'all', source: 'domain_scope' },
  'L4 has explicit "all" on expenses',
);
eq(
  resolveScope({ roleScopes, roleFallback, domainId: 'expenses', roleId: 'L2B' }),
  { kind: 'department', source: 'role_default' },
  'L2B falls back to role default (department)',
);
eq(
  resolveScope({ roleScopes, roleFallback, domainId: 'audit', roleId: 'L2A' }),
  { kind: 'self', source: 'role_default' },
  'L2A without domain_scope row uses role default (self)',
);

console.log('visibility: deny is explicit and wins over inheritance');

const denyScopes = { L1: { expenses: 'deny' } };
eq(
  resolveScope({ roleScopes: denyScopes, roleFallback, domainId: 'expenses', roleId: 'L1' }),
  { kind: 'deny', source: 'domain_scope' },
  'deny from domain_scope is the resolved kind',
);

console.log('visibility: label composition');

const teamNames = { 'team-backend': 'Backend', 'team-data': 'Data' };
eq(
  labelForScope('team', ['team-backend', 'team-data'], teamNames, 'self', null),
  'Backend, Data',
  'team label joins team group names',
);
eq(
  labelForScope('team', [], {}, 'self', null),
  'Team (no teams assigned)',
  'team with empty ids shows fallback',
);
eq(
  labelForScope('department', [], {}, 'self', 'Engineering'),
  'Dept · Engineering',
  'department label uses actor department',
);
eq(
  labelForScope('self', [], {}, 'self', null),
  'Self only',
  'self label is literal',
);
eq(
  labelForScope('all', [], {}, 'self', null),
  'All (company-wide)',
  'all label is literal',
);

console.log('notifications: event → domain mapping');

eq(mapEventToDomain('expense.submitted', 'expense'), 'expenses', 'expense.* → expenses via refType');
eq(mapEventToDomain('pr.approved',       'pr'),       'pr',       'pr.* → pr');
eq(mapEventToDomain('po.issued',         'po'),       'po',       'po.* → po');
eq(mapEventToDomain('slip.uploaded',     'slip'),     'slips',    'slip.* → slips');
eq(mapEventToDomain('user.role_changed', 'user'),     'users',    'user.* → users');
eq(mapEventToDomain('audit.override',    null),       'audit',    'audit.* → audit without refType');
eq(mapEventToDomain('ai.invoke',         null),       'ai_settings', 'ai.* → ai_settings');
eq(mapEventToDomain('notification.cleared', null),    'notifications', 'notification.* → notifications');
eq(mapEventToDomain('notif.digest',      null),       'notifications', 'notif.* → notifications (alias)');
eq(mapEventToDomain('unknown.event',     null),       null,       'unknown event type returns null');

console.log('visibility: deny short-circuits fanout');

function shouldFanout(scopeKind) {
  return scopeKind !== 'deny' && scopeKind !== 'self';
}
eq(shouldFanout('deny'),       false, 'deny is excluded from fanout');
eq(shouldFanout('self'),       false, 'self is excluded from fanout');
eq(shouldFanout('department'), true,  'department fans out');
eq(shouldFanout('team'),       true,  'team fans out');
eq(shouldFanout('all'),        true,  'all fans out');
eq(shouldFanout('subtree'),    true,  'subtree fans out');

console.log(`\nvisibility: ${pass} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
