// Pure-logic tests for the inheritance resolver used by
//   app/src/components/org-chart/useDirty.ts:optimisticState
// and the server-side
//   rbac/src/lib/inheritance.ts:resolveCell
//
// We re-implement the optimistic resolver here to avoid pulling in React/Next.
// If useDirty.ts changes, update the mirror below.

// --- Mirror of optimisticState (post-change) --------------------------------
function resolveOptimistic({ role, mod, action, matrix, dirty, parentMap }) {
  const k = `${role}|${mod}|${action}`;
  const d = dirty.get(k);
  const live = matrix.rows.find((r) => r.module_id === mod)?.cells?.[role]?.[action];
  if (!d) return live ?? { state: 'deny', source: 'default' };
  if (d.next === 'allow' || d.next === 'deny') {
    return { state: d.next, source: 'explicit' };
  }
  // d.next === 'inherit' — walk the parent chain.
  if (parentMap) {
    let cursor = parentMap.get(role) ?? null;
    while (cursor) {
      const ancestorLive = matrix.rows.find((r) => r.module_id === mod)?.cells?.[cursor]?.[action];
      if (ancestorLive && (ancestorLive.state === 'allow' || ancestorLive.state === 'deny')) {
        return {
          state: ancestorLive.state,
          source: 'inherited_from_parent',
          inheritedFrom: cursor,
        };
      }
      cursor = parentMap.get(cursor) ?? null;
    }
  }
  return { state: 'deny', source: 'default' };
}

let pass = 0;
let fail = 0;
function check(name, got, want) {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (ok) { pass++; console.log('✓', name, '->', JSON.stringify(got)); }
  else { fail++; console.log('✗', name, 'want', JSON.stringify(want), 'got', JSON.stringify(got)); }
}

// --- Fixture: HQ → L4 → L3 --------------------------------------------------
//   L4 has explicit allow on budget-finance:read.
//   No one has anything on knowledge-base.
const parentMap = new Map([
  ['HQ', null],
  ['L4', 'HQ'],
  ['L3', 'L4'],
]);

const matrix = {
  modules: [
    { id: 'budget-finance', display_name: 'Budget & Finance', group_name: 'finance', sort_order: 1, allowed_actions: ['read'] },
    { id: 'knowledge-base', display_name: 'Knowledge Base',  group_name: 'kb',      sort_order: 2, allowed_actions: ['read'] },
  ],
  columns: [
    { id: 'HQ', name: 'HQ', level: 1, parent_id: null, sort_order: 1, is_system: true },
    { id: 'L4', name: 'L4', level: 2, parent_id: 'HQ', sort_order: 2, is_system: false },
    { id: 'L3', name: 'L3', level: 3, parent_id: 'L4', sort_order: 3, is_system: false },
  ],
  rows: [
    {
      module_id: 'budget-finance',
      // Sparse: only L4 has a live cell. L3 and HQ have no entry.
      cells: {
        L4: { read: { state: 'allow', source: 'explicit' } },
      },
    },
    {
      module_id: 'knowledge-base',
      // No live cells anywhere — no one has it.
      cells: {},
    },
  ],
};

const noDirty = new Map();

// 1. L4 + budget-finance + read + dirty=allow → { state: 'allow', source: 'explicit' }
{
  const dirty = new Map([
    ['L4|budget-finance|read', { role: 'L4', module: 'budget-finance', action: 'read', next: 'allow' }],
  ]);
  check('L4 explicit allow', resolveOptimistic({
    role: 'L4', mod: 'budget-finance', action: 'read', matrix, dirty, parentMap,
  }), { state: 'allow', source: 'explicit' });
}

// 2. L3 + budget-finance + read + dirty=inherit → walks to L4 → inherited_from_parent
{
  const dirty = new Map([
    ['L3|budget-finance|read', { role: 'L3', module: 'budget-finance', action: 'read', next: 'inherit' }],
  ]);
  check('L3 inherits allow from L4', resolveOptimistic({
    role: 'L3', mod: 'budget-finance', action: 'read', matrix, dirty, parentMap,
  }), { state: 'allow', source: 'inherited_from_parent', inheritedFrom: 'L4' });
}

// 3. L3 + knowledge-base + read + dirty=inherit (no ancestor has it) → deny/default
{
  const dirty = new Map([
    ['L3|knowledge-base|read', { role: 'L3', module: 'knowledge-base', action: 'read', next: 'inherit' }],
  ]);
  check('L3 inherit falls back when chain empty', resolveOptimistic({
    role: 'L3', mod: 'knowledge-base', action: 'read', matrix, dirty, parentMap,
  }), { state: 'deny', source: 'default' });
}

// 4. L3 + budget-finance + read + no dirty entry → live matrix value or deny/default
//    Here L3 has no live cell, so we expect the deny/default fallback.
check('L3 no-dirty falls back to deny/default', resolveOptimistic({
  role: 'L3', mod: 'budget-finance', action: 'read', matrix, dirty: noDirty, parentMap,
}), { state: 'deny', source: 'default' });

// Bonus: when no parentMap is supplied, inherit falls back to deny/default (graceful).
{
  const dirty = new Map([
    ['L3|budget-finance|read', { role: 'L3', module: 'budget-finance', action: 'read', next: 'inherit' }],
  ]);
  check('inherit without parentMap -> deny/default', resolveOptimistic({
    role: 'L3', mod: 'budget-finance', action: 'read', matrix, dirty, parentMap: undefined,
  }), { state: 'deny', source: 'default' });
}

// Bonus: L4 explicit deny is also inherited (state 'deny' from ancestor still wins).
{
  const localMatrix = {
    ...matrix,
    rows: [
      {
        module_id: 'budget-finance',
        cells: {
          L4: { read: { state: 'deny', source: 'explicit' } },
        },
      },
      matrix.rows[1],
    ],
  };
  const dirty = new Map([
    ['L3|budget-finance|read', { role: 'L3', module: 'budget-finance', action: 'read', next: 'inherit' }],
  ]);
  check('L3 inherits deny from L4', resolveOptimistic({
    role: 'L3', mod: 'budget-finance', action: 'read', matrix: localMatrix, dirty, parentMap,
  }), { state: 'deny', source: 'inherited_from_parent', inheritedFrom: 'L4' });
}

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
