// Pure-logic tests for the Linux-style group cascade resolution.
// Mirrors lib/perm/inheritance.ts:resolveCellWithGroups.
//
// Run: node app/tests/group-cascade.test.mjs

// Mirror of the cascade resolution with a precomputed allow map (no DB).

/** @typedef {'allow'|'deny'} EffectiveState */
/** @typedef {'explicit'|'inherited_from_parent'|'group_cascade'|'default'} Source */

/**
 * @param {{roleId:string, moduleId:string, action:string}} q
 * @param {Record<string, Record<string, EffectiveState>>} direct         // direct[role][module] = state
 * @param {Record<string, string[]>} moduleGroups                          // moduleGroups[module] = groupIds[]
 * @param {Record<string, string|null>} groupParents                        // groupParents[group] = parentGroup|null
 * @param {Record<string, string[]>} roleGroups                            // roleGroups[role] = groupIds[]
 * @param {Record<string, Record<string, Record<string, EffectiveState>>>} groupPerms  // groupPerms[role][group][action] = state
 * @returns {{state: EffectiveState, source: Source, inheritedFrom?: string}}
 */
function resolveCellWithGroups(q, direct, moduleGroups, groupParents, roleGroups, groupPerms) {
  // 1. Direct module ACL (explicit)
  const own = direct[q.roleId]?.[q.moduleId];
  if (own === 'allow' || own === 'deny') {
    return { state: own, source: 'explicit' };
  }

  // 2. Group cascade: walk module → groups → ancestor groups → role memberships → group_perms
  const memberGroupIds = roleGroups[q.roleId] ?? [];
  const seedGroups = (moduleGroups[q.moduleId] ?? []).filter((g) => memberGroupIds.includes(g));

  // Walk parent chain for each seed group
  const visited = new Set();
  const groups = [];
  for (const g of seedGroups) {
    let cursor = g;
    while (cursor && !visited.has(cursor)) {
      visited.add(cursor);
      groups.push(cursor);
      cursor = groupParents[cursor] ?? null;
    }
  }

  for (const g of groups) {
    const state = groupPerms[q.roleId]?.[g]?.[q.action];
    if (state === 'allow') return { state: 'allow', source: 'group_cascade', inheritedFrom: g };
    if (state === 'deny')  return { state: 'deny',  source: 'group_cascade', inheritedFrom: g };
  }

  // 3. Default deny
  return { state: 'deny', source: 'default' };
}

// --- Fixtures ----------------------------------------------------------------

const moduleGroups = {
  'tile-submit-expense':  ['grp-workflow'],
  'tile-approve-expense': ['grp-workflow-approval'],
  'tile-cockpit':         ['grp-cockpit'],
  'tile-ledger':          ['grp-finance'],
  'tile-departments':     ['grp-hr'],
  'tile-policy':          ['grp-policy'],
};

const groupParents = {
  'grp-workflow':          null,
  'grp-workflow-approval': 'grp-workflow',
  'grp-finance':           null,
  'grp-cockpit':           null,
  'grp-hr':                null,
  'grp-policy':            null,
};

const roleGroups = {
  L4: ['grp-workflow', 'grp-workflow-approval', 'grp-finance', 'grp-cockpit', 'grp-hr', 'grp-policy', 'grp-it'],
  L3: ['grp-workflow-approval', 'grp-workflow', 'grp-finance', 'grp-hr'],
  L2B:['grp-workflow', 'grp-workflow-approval', 'grp-finance'],
  L2A:['grp-workflow', 'grp-hr'],
};

const groupPerms = {
  L4: {
    'grp-workflow':          { read: 'allow', create: 'allow', update: 'allow', delete: 'allow' },
    'grp-workflow-approval': { read: 'allow', create: 'allow', update: 'allow', delete: 'allow' },
    'grp-finance':           { read: 'allow', create: 'allow', update: 'allow', delete: 'allow' },
    'grp-cockpit':           { read: 'allow', create: 'allow', update: 'allow', delete: 'allow' },
    'grp-hr':                { read: 'allow', create: 'allow', update: 'allow', delete: 'allow' },
    'grp-policy':            { read: 'allow', create: 'allow', update: 'allow', delete: 'allow' },
    'grp-it':                { read: 'allow', create: 'allow', update: 'allow', delete: 'allow' },
  },
  L3: {
    'grp-workflow-approval': { read: 'allow', update: 'allow' },
    'grp-workflow':          { read: 'allow', update: 'allow' },
    'grp-finance':           { read: 'allow', update: 'allow' },
    'grp-hr':                { read: 'allow', update: 'allow' },
  },
  L2B: {
    'grp-workflow':          { read: 'allow', update: 'allow' },
    'grp-workflow-approval': { read: 'allow', update: 'allow' },
    'grp-finance':           { read: 'allow', update: 'allow' },
  },
  L2A: {
    'grp-workflow':          { read: 'allow', create: 'allow' },
    'grp-hr':                { read: 'allow' },
  },
};

const direct = {}; // no explicit overrides → all goes through cascade

// --- Test cases --------------------------------------------------------------

const cases = [
  // L4 (admin/cfo/ceo/it) gets everything
  { role: 'L4', module: 'tile-submit-expense',  action: 'read', expect: { state: 'allow', source: 'group_cascade' } },
  { role: 'L4', module: 'tile-approve-expense', action: 'update', expect: { state: 'allow', source: 'group_cascade' } },
  { role: 'L4', module: 'tile-cockpit',         action: 'read', expect: { state: 'allow', source: 'group_cascade' } },
  { role: 'L4', module: 'tile-departments',     action: 'update', expect: { state: 'allow', source: 'group_cascade' } },

  // L3 (manager/hod/accounting_manager): workflow-approval + workflow + finance + hr
  { role: 'L3', module: 'tile-approve-expense', action: 'read', expect: { state: 'allow', source: 'group_cascade' } },
  { role: 'L3', module: 'tile-cockpit',         action: 'read', expect: { state: 'deny',  source: 'default' } },
  { role: 'L3', module: 'tile-departments',     action: 'read', expect: { state: 'allow', source: 'group_cascade' } },
  { role: 'L3', module: 'tile-policy',          action: 'read', expect: { state: 'deny',  source: 'default' } },

  // L2B (supervisor/account_officer): workflow-approval + workflow + finance
  { role: 'L2B', module: 'tile-approve-expense', action: 'read', expect: { state: 'allow', source: 'group_cascade' } },
  { role: 'L2B', module: 'tile-departments',     action: 'read', expect: { state: 'deny',  source: 'default' } },

  // L2A (staff/accountant/account_supervisor/hr/hr_manager): workflow + hr
  { role: 'L2A', module: 'tile-submit-expense',  action: 'read', expect: { state: 'allow', source: 'group_cascade' } },
  { role: 'L2A', module: 'tile-submit-expense',  action: 'create', expect: { state: 'allow', source: 'group_cascade' } },
  { role: 'L2A', module: 'tile-submit-expense',  action: 'delete', expect: { state: 'deny',  source: 'default' } },
  { role: 'L2A', module: 'tile-departments',     action: 'read', expect: { state: 'allow', source: 'group_cascade' } },
  { role: 'L2A', module: 'tile-departments',     action: 'update', expect: { state: 'deny',  source: 'default' } }, // hr only allows read for L2A
  { role: 'L2A', module: 'tile-approve-expense', action: 'read', expect: { state: 'deny',  source: 'default' } },
  { role: 'L2A', module: 'tile-cockpit',         action: 'read', expect: { state: 'deny',  source: 'default' } },

  // Parent chain: L3 in grp-workflow-approval cascades to grp-workflow parent.
  // The seed allows grp-workflow-approval but not grp-workflow → first match wins.
  // (For L3 specifically we seed grp-workflow with read+update too.)

  // Explicit override beats cascade.
  {
    role: 'L2A',
    module: 'tile-submit-expense',
    action: 'read',
    directOverride: { L2A: { 'tile-submit-expense': 'deny' } },
    expect: { state: 'deny', source: 'explicit' },
  },
];

let pass = 0, fail = 0;
for (const tc of cases) {
  const overrides = tc.directOverride ? { ...direct, ...tc.directOverride } : direct;
  const got = resolveCellWithGroups(
    { roleId: tc.role, moduleId: tc.module, action: tc.action },
    overrides,
    moduleGroups,
    groupParents,
    roleGroups,
    groupPerms,
  );
  const stateOk = got.state === tc.expect.state;
  const sourceOk = !tc.expect.source || got.source === tc.expect.source;
  if (stateOk && sourceOk) {
    pass++;
    console.log(`✓ ${tc.role.padEnd(4)} / ${tc.module.padEnd(24)} / ${tc.action.padEnd(6)} → ${got.state} (${got.source}${got.inheritedFrom ? ' via ' + got.inheritedFrom : ''})`);
  } else {
    fail++;
    console.log(`✗ ${tc.role.padEnd(4)} / ${tc.module.padEnd(24)} / ${tc.action.padEnd(6)} → ${got.state} (${got.source}), want ${tc.expect.state} (${tc.expect.source})`);
  }
}

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail > 0 ? 1 : 0);