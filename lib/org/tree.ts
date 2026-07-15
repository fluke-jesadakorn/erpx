// Derived org tree. The reporting hierarchy used by the org chart in SignInPanel,
// by orgScope, by notifications routing, and by the auto-wire suggestion engine
// is computed on demand from these sources:
//
//   1. perm.user_effective_level (derived from rbac:level:grant:min:N:all perms
//      held via perm.user_roles; falls back to 10 = lowest authority)
//
//   2. users.dept_group_id (FK to perm.roles where kind='department')
//
//   3. perm.roles.sort_order (tiebreak for "most senior persona")
//
//   4. The "executive" department is the anchor: everyone in the top tier
//      (effective_level <= 2) lives there and links up to the level-1 user
//      (the CEO) as the single root.
//
// Rules (in order):
//   - CEO (effective_level = 1) is a root.
//   - Other users in dept-executive at level 2 report to the CEO.
//   - Users in dept-executive at level >= 3 report to the closest lower-level
//     user in the same dept.
//   - Dept heads of non-executive depts (lowest effective_level in dept,
//     tiebreak by persona sort_order ASC, then id ASC) report to the most
//     senior C-level in dept-executive (lowest sort_order at level 2).
//   - Non-heads in a non-executive dept report to the closest lower-level
//     user in the same dept (tiebreak by sort_order ASC, then id ASC).
//   - Users with no dept report to the CEO.
//   - Fallback: null (root).

export interface DerivedUser {
  id: number;
  employee_code: string;
  fullname: string;
  dept_id: string | null;
  effective_level: number;
  primary_persona: string;
  persona_sort: number;
}

export interface TreeNode {
  user: DerivedUser;
  children: TreeNode[];
}

const EXEC_DEPT = 'executive';

function bySeniority(a: DerivedUser, b: DerivedUser): number {
  if (a.effective_level !== b.effective_level) return a.effective_level - b.effective_level;
  const aSort = a.persona_sort ?? 999;
  const bSort = b.persona_sort ?? 999;
  if (aSort !== bSort) return aSort - bSort;
  return a.id - b.id;
}

function findCeo(users: DerivedUser[]): DerivedUser | null {
  return users.find((u) => u.effective_level === 1) ?? null;
}

function findExecLevel2(users: DerivedUser[]): DerivedUser | null {
  const candidates = users
    .filter((u) => normalizeDept(u.dept_id) === EXEC_DEPT && u.effective_level === 2)
    .sort(bySeniority);
  return candidates[0] ?? null;
}

function normalizeDept(d: string | null | undefined): string | null {
  if (d == null) return null;
  const t = String(d).trim();
  if (t === '' || t === 'null' || t === 'undefined') return null;
  return t;
}

function isDeptHead(u: DerivedUser, deptUsers: DerivedUser[]): boolean {
  const sorted = [...deptUsers].sort(bySeniority);
  return sorted[0]?.id === u.id;
}

function closestLowerInDept(
  u: DerivedUser,
  deptUsers: DerivedUser[]
): DerivedUser | null {
  const targetLevel = u.effective_level - 1;
  if (targetLevel >= 1) {
    const direct = deptUsers
      .filter((m) => m.id !== u.id && m.effective_level === targetLevel)
      .sort(bySeniority);
    if (direct.length > 0) return direct[0];
  }
  const candidates = deptUsers
    .filter((m) => m.id !== u.id && m.effective_level < u.effective_level)
    .sort(bySeniority);
  return candidates[0] ?? null;
}

export function deriveManager(
  u: DerivedUser,
  all: DerivedUser[],
  ceo: DerivedUser | null,
  execLevel2: DerivedUser | null
): number | null {
  const dept = normalizeDept(u.dept_id);
  if (u.effective_level === 1) return null;

  if (dept === EXEC_DEPT) {
    if (u.effective_level === 2) return ceo?.id ?? null;
    const inExec = all.filter((m) => normalizeDept(m.dept_id) === EXEC_DEPT);
    return closestLowerInDept(u, inExec)?.id ?? ceo?.id ?? null;
  }

  if (dept == null) {
    return ceo?.id ?? null;
  }

  const deptUsers = all.filter((m) => normalizeDept(m.dept_id) === dept);
  if (isDeptHead(u, deptUsers)) {
    return execLevel2?.id ?? ceo?.id ?? null;
  }
  return closestLowerInDept(u, deptUsers)?.id ?? execLevel2?.id ?? ceo?.id ?? null;
}

export function buildDerivedForest(users: DerivedUser[]): TreeNode[] {
  const ceo = findCeo(users);
  const execLevel2 = findExecLevel2(users);
  const map = new Map<number, TreeNode>();
  users.forEach((u) => map.set(u.id, { user: u, children: [] }));
  const roots: TreeNode[] = [];
  users.forEach((u) => {
    const node = map.get(u.id)!;
    const parentId = deriveManager(u, users, ceo, execLevel2);
    if (parentId == null || !map.has(parentId)) {
      roots.push(node);
    } else {
      map.get(parentId)!.children.push(node);
    }
  });
  const sortRec = (n: TreeNode) => {
    n.children.sort((a, b) => bySeniority(a.user, b.user));
    n.children.forEach(sortRec);
  };
  roots.sort((a, b) => bySeniority(a.user, b.user));
  roots.forEach(sortRec);
  return roots;
}

export function annotateDerived(
  users: Array<Omit<DerivedUser, 'effective_level' | 'primary_persona' | 'persona_sort'> & {
    effective_level: number;
    primary_persona: string;
    persona_sort: number;
  }>
): Array<DerivedUser & { derived_manager_id: number | null }> {
  const ceo = findCeo(users);
  const execLevel2 = findExecLevel2(users);
  return users.map((u) => ({
    ...u,
    derived_manager_id: deriveManager(u, users, ceo, execLevel2),
  }));
}
