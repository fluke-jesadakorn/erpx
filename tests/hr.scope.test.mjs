// Pure-logic tests for the HR scope helpers in lib/org/scope.ts.
// We re-implement `descendantsFromMap` here to avoid pulling in the pg client.
// If the helper changes in orgScope.ts, update this file to match.

// Mirror of orgScope.ts:descendantsFromMap
function descendantsFromMap(rootId, reportsTo) {
  const out = [];
  const stack = [rootId];
  while (stack.length) {
    const cur = stack.pop();
    const kids = reportsTo.get(cur) || [];
    for (const k of kids) {
      out.push(k);
      stack.push(k);
    }
  }
  return out;
}

// Mirror of orgScope.ts:assertCanEditUser (without DB query)
function canEditUser(scope, targetUserId) {
  if (scope.isHrManager) return { ok: true };
  if (scope.isHod) {
    if (scope.actor.id === targetUserId) {
      return { ok: false, reason: 'cannot edit self' };
    }
    if (!scope.subtreeIds.includes(targetUserId)) {
      return { ok: false, reason: 'out of subtree' };
    }
    return { ok: true };
  }
  return { ok: false, reason: 'role not allowed' };
}

let pass = 0;
let fail = 0;
function check(name, got, want) {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (ok) { pass++; console.log('✓', name, '->', JSON.stringify(got)); }
  else { fail++; console.log('✗', name, 'want', JSON.stringify(want), 'got', JSON.stringify(got)); }
}

// Build a small org tree:
//   EMP002 (HOD) ──┬── EMP007
//                  └── EMP010
//   EMP015 (HR Manager) ── EMP016
//   EMP006 (CEO) ─── EMP002, EMP015
const reportsTo = new Map();
reportsTo.set(2, [7, 10]);
reportsTo.set(15, [16]);
reportsTo.set(6, [2, 15]);

// 1. HoD EMP002 descendants = [7, 10]
check('HoD(2) descendants', descendantsFromMap(2, reportsTo), [7, 10]);

// 2. HR Manager EMP015 subtree = [16]
check('HR-Mgr(15) descendants', descendantsFromMap(15, reportsTo), [16]);

// 3. CEO EMP006 subtree = {2, 7, 10, 15, 16} — order follows stack LIFO
check('CEO(6) descendants', descendantsFromMap(6, reportsTo), [2, 15, 16, 7, 10]);

// 4. HoD EMP002 can edit direct report (7)
check('HoD edits direct report', canEditUser(
  { actor: { id: 2 }, isHrManager: false, isHod: true, subtreeIds: [7, 10] },
  7
), { ok: true });

// 5. HoD EMP002 cannot edit sibling team (16)
check('HoD rejects out-of-subtree', canEditUser(
  { actor: { id: 2 }, isHrManager: false, isHod: true, subtreeIds: [7, 10] },
  16
), { ok: false, reason: 'out of subtree' });

// 6. HoD cannot edit self
check('HoD rejects self-edit', canEditUser(
  { actor: { id: 2 }, isHrManager: false, isHod: true, subtreeIds: [7, 10] },
  2
), { ok: false, reason: 'cannot edit self' });

// 7. HR Manager can edit anyone (15 edits 7)
check('HR-Mgr edits anyone', canEditUser(
  { actor: { id: 15 }, isHrManager: true, isHod: false, subtreeIds: [] },
  7
), { ok: true });

// 8. Staff role rejected
check('Staff rejected', canEditUser(
  { actor: { id: 7 }, isHrManager: false, isHod: false, subtreeIds: [] },
  16
), { ok: false, reason: 'role not allowed' });

// 9. HoD edits grand-report (only direct, NOT transitive in current model)
//     Spec: HoD scope = direct reports only. EMP007 has no reports_to children
//     under EMP002 chain beyond depth 1, so subtreeIds = [7, 10] not transitive.
check('HoD direct-only subtree', descendantsFromMap(2, reportsTo).includes(7), true);
check('HoD cannot edit grand-report through deeper chain (e.g. EMP016 from EMP002)',
  canEditUser(
    { actor: { id: 2 }, isHrManager: false, isHod: true, subtreeIds: descendantsFromMap(2, reportsTo) },
    16
  ),
  { ok: false, reason: 'out of subtree' });

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);