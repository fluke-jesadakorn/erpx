// Pure-logic tests for computeRecipients() domain broadcast.
//
// We re-implement the policy decisions as mirrors:
//   1. NO_FANOUT_TYPES short-circuits
//   2. Ref-owner / actor / supervisor are always considered
//   3. Domain broadcast layer adds ids based on role scope_kind
//
// The real implementation is in
//   app/src/lib/notifications/recipients.ts
// and queries Postgres directly. These tests assert the
// decision logic, not the SQL.

let pass = 0;
let fail = 0;
function eq(actual, expected, msg) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) { pass++; console.log(`  ✓ ${msg}`); }
  else { fail++; console.log(`  ✗ ${msg}\n    expected: ${e}\n    actual:   ${a}`); }
}

const NO_FANOUT_TYPES = new Set(['policy.updated']);

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

// Mirror: gather the set of candidate user ids by walking the
// (role, scope_kind) matrix and applying per-scope filter logic.
function broadcast(domainId, scopeRows, getUsers) {
  const ids = new Set();
  for (const row of scopeRows) {
    if (row.scope_kind === 'deny' || row.scope_kind === 'self') continue;
    const matched = getUsers(row);
    for (const id of matched) ids.add(id);
  }
  return [...ids];
}

console.log('recipients: policy.updated is silenced');
eq(NO_FANOUT_TYPES.has('policy.updated'), true, 'policy.updated is in NO_FANOUT_TYPES');

console.log('recipients: event → domain mapping');
eq(mapEventToDomain('expense.submitted', null), 'expenses', 'expense.* maps to expenses');
eq(mapEventToDomain('pr.submitted', 'pr'),       'pr',       'pr refType pinned to pr domain');
eq(mapEventToDomain('po.issued', 'po'),          'po',       'po refType pinned to po domain');
eq(mapEventToDomain('slip.uploaded', null),      'slips',    'slip.* maps to slips');

console.log('recipients: deny scope excludes broadcast');

const rows = [
  { rbac_role_id: 'L4', scope_kind: 'all' },
  { rbac_role_id: 'L1', scope_kind: 'deny' },
  { rbac_role_id: 'L3', scope_kind: 'department' },
];
const ids = broadcast('expenses', rows, (r) => {
  if (r.scope_kind === 'all') return [100, 200, 300];
  if (r.scope_kind === 'department') return [200, 400];
  return [];
});
eq(ids.sort(), [100, 200, 300, 400], 'all + department broadcast, deny excluded');

console.log('recipients: only-self roles do not broadcast');
const onlySelf = [{ rbac_role_id: 'L2A', scope_kind: 'self' }];
eq(broadcast('expenses', onlySelf, () => [1, 2]), [], 'self is excluded from fanout');

console.log('recipients: empty domain scope (no rows) means no broadcast');
eq(broadcast('expenses', [], () => [1, 2, 3]), [], 'no scope rows = no broadcast');

console.log(`\nrecipients-domain: ${pass} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
