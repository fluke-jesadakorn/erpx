import 'server-only';
import { query } from '@/db';
import { ROLE_DOMAIN, type DisplayRoleName } from '@/org/display';

export interface UserForWiring {
  id: number;
  employee_code: string;
  fullname: string;
  role_name: string;
  dept_id: number | null;
  dept_code: string | null;
  dept_name: string | null;
  level: number;
  reports_to_user_id: number | null;
  is_active: boolean;
}

export interface WireEdge {
  userId: number;
  managerId: number;
  isNew: boolean;
  isRoot: boolean;
}

export interface ProposedTreeNode {
  user: UserForWiring;
  children: ProposedTreeNode[];
  proposedManagerId: number | null;
  currentManagerId: number | null;
  isNewWire: boolean;
}

export interface AutoWireProposal {
  roots: ProposedTreeNode[];
  flat: ProposedTreeNode[];
  wires: WireEdge[];
  stats: {
    totalNodes: number;
    newWires: number;
    unchangedWires: number;
    maxDepth: number;
    avgChildren: number;
  };
}

const SOFT_CHILD_CAP = 6;

function domainOf(roleName: string | undefined): string {
  return ROLE_DOMAIN[roleName as DisplayRoleName] || 'general';
}

function scoreEdge(
  parent: UserForWiring,
  child: UserForWiring,
  siblingCount: number,
  childsCurrentParentId: number | null,
): number {
  if (parent.id === child.id) return Number.POSITIVE_INFINITY;

  // Same-level wires are allowed but penalized (e.g. CFO→CEO, CoPeers).
  const levelGap = child.level - parent.level;
  if (levelGap < 0) return Number.POSITIVE_INFINITY;

  let s = 0;

  // Department: prefer different department
  const sameDept = !!(parent.dept_id && child.dept_id && parent.dept_id === child.dept_id);
  if (sameDept) s += 50;

  // Domain: prefer matching domain (penalize cross-domain), unless parent is general/exec
  const parentDomain = domainOf(parent.role_name);
  const childDomain = domainOf(child.role_name);
  if (
    parentDomain !== 'general' &&
    parentDomain !== 'exec' &&
    parentDomain !== childDomain
  ) {
    s += 80;
  }

  // Level gap: gap=1 ideal; skip-level costs more
  if (levelGap === 0) s += 5; // small cost for same-level
  else if (levelGap > 1) s += 20 * (levelGap - 1);

  // Crowding penalty
  if (siblingCount >= SOFT_CHILD_CAP) s += 200;

  // Stability bonus: keep current parent if it's reasonable (gap <= 2 and not crossing domains badly)
  if (childsCurrentParentId === parent.id) s -= 30;

  return s;
}

function createsCycle(usersById: Map<number, UserForWiring>, childId: number, candidateParentId: number): boolean {
  if (childId === candidateParentId) return true;
  let cur: number | null = candidateParentId;
  const seen = new Set<number>();
  while (cur != null) {
    if (cur === childId) return true;
    if (seen.has(cur)) return true;
    seen.add(cur);
    cur = usersById.get(cur)?.reports_to_user_id ?? null;
  }
  return false;
}

export async function loadAllUsersForWiring(): Promise<UserForWiring[]> {
  const r = await query(
    `SELECT u.id, u.employee_code, u.fullname,
            u.is_active,
            COALESCE((
              SELECT split_part(ur.role_id, '::', 2)::int
                FROM perm.user_roles ur
               WHERE ur.user_id = u.id
               ORDER BY (CASE WHEN ur.role_id LIKE '%::1' THEN 0
                              WHEN ur.role_id LIKE '%::2' THEN 1
                              WHEN ur.role_id LIKE '%::3' THEN 2
                              WHEN ur.role_id LIKE '%::4' THEN 3
                              WHEN ur.role_id LIKE '%::5' THEN 4
                              ELSE 5 END), ur.granted_at ASC
               LIMIT 1
            ), 5) AS level,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id
              ORDER BY (CASE WHEN ur.role_id LIKE '%::1' THEN 0
                             WHEN ur.role_id LIKE '%::2' THEN 1
                             WHEN ur.role_id LIKE '%::3' THEN 2
                             WHEN ur.role_id LIKE '%::4' THEN 3
                             WHEN ur.role_id LIKE '%::5' THEN 4
                             ELSE 5 END), ur.granted_at ASC
              LIMIT 1) AS role_id,
            (SELECT up.permission_id FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS dept_perm
       FROM users u
       ORDER BY COALESCE(
         (SELECT split_part(ur.role_id, '::', 2)::int FROM perm.user_roles ur
           WHERE ur.user_id = u.id LIMIT 1), 5), u.id`
  );
  return r.rows.map((row: any) => {
    const deptRaw: string | null = row.dept_perm ?? null;
    const deptId = deptRaw ? deptRaw.replace(/^user:dept:/, '').replace(/::allow$/, '') : null;
    const roleName = row.role_id ? row.role_id.split('::')[0] : null;
    return {
      id: row.id,
      employee_code: row.employee_code,
      fullname: row.fullname,
      role_name: roleName,
      dept_id: deptId as any,
      dept_code: null,
      dept_name: deptId,
      level: typeof row.level === 'number' ? row.level : 5,
      reports_to_user_id: null,
      is_active: row.is_active,
    };
  });
}

export async function proposeAutoWire(): Promise<AutoWireProposal> {
  const users = await loadAllUsersForWiring();
  if (users.length === 0) {
    return {
      roots: [],
      flat: [],
      wires: [],
      stats: { totalNodes: 0, newWires: 0, unchangedWires: 0, maxDepth: 0, avgChildren: 0 },
    };
  }

  const usersById = new Map(users.map((u) => [u.id, u]));
  const ceo = users.find((u) => u.role_name === 'ceo');
  const root = ceo ?? users.reduce((min, u) => (u.level < min.level ? u : min));

  const proposedWires = new Map<number, number>();
  const childCount = new Map<number, number>();

  for (const u of users) {
    if (u.id === root.id) continue;
    if (u.reports_to_user_id == null) continue;
    const parent = usersById.get(u.reports_to_user_id);
    if (!parent) continue;
    if (createsCycle(usersById, u.id, parent.id)) continue;
    // Preserve chain only if it's a valid edge (not same-level unless CEO)
    const isCtoCEO = parent.role_name === 'ceo';
    if (!isCtoCEO && parent.level > u.level) continue;
    proposedWires.set(u.id, parent.id);
    childCount.set(parent.id, (childCount.get(parent.id) ?? 0) + 1);
  }

  const byLevel = new Map<number, UserForWiring[]>();
  for (const u of users) {
    const arr = byLevel.get(u.level) ?? [];
    arr.push(u);
    byLevel.set(u.level, arr);
  }

  const eligibleParents = new Set<number>([root.id, ...proposedWires.keys()]);

  for (const lv of [1, 2, 3, 4, 5]) {
    const tier = byLevel.get(lv) ?? [];
    for (const u of tier) {
      if (u.id === root.id) continue;
      if (proposedWires.has(u.id)) continue;
      let best: UserForWiring | null = null;
      let bestScore = Number.POSITIVE_INFINITY;
      for (const pId of eligibleParents) {
        const p = usersById.get(pId);
        if (!p) continue;
        // Reject if parent is at higher level number (lower rank)
        if (p.level > u.level) continue;
        // Same-level only allowed if parent is CEO
        if (p.level === u.level && p.role_name !== 'ceo') continue;
        const score = scoreEdge(p, u, childCount.get(p.id) ?? 0, u.reports_to_user_id);
        if (score < bestScore) {
          bestScore = score;
          best = p;
        }
      }
      if (best) {
        proposedWires.set(u.id, best.id);
        childCount.set(best.id, (childCount.get(best.id) ?? 0) + 1);
        eligibleParents.add(u.id);
      } else {
        eligibleParents.add(u.id);
      }
    }
  }

  const wires: WireEdge[] = [];
  let newWires = 0;
  let unchangedWires = 0;
  for (const u of users) {
    if (u.id === root.id) continue;
    const proposed = proposedWires.get(u.id) ?? null;
    const current = u.reports_to_user_id;
    const isNew = proposed !== current;
    if (proposed != null) {
      wires.push({ userId: u.id, managerId: proposed, isNew, isRoot: false });
    }
    if (isNew) newWires++;
    else unchangedWires++;
  }

  const childrenByParent = new Map<number, number[]>();
  for (const [userId, managerId] of proposedWires.entries()) {
    const arr = childrenByParent.get(managerId) ?? [];
    arr.push(userId);
    childrenByParent.set(managerId, arr);
  }

  function buildNode(userId: number, depth: number): ProposedTreeNode {
    const u = usersById.get(userId)!;
    const childIds = childrenByParent.get(userId) ?? [];
    const proposed = proposedWires.get(userId) ?? null;
    return {
      user: u,
      children: childIds
        .map((cid) => buildNode(cid, depth + 1))
        .sort((a, b) => {
          if (a.user.level !== b.user.level) return a.user.level - b.user.level;
          return (a.user.fullname || '').localeCompare(b.user.fullname || '');
        }),
      proposedManagerId: proposed,
      currentManagerId: u.reports_to_user_id,
      isNewWire: u.id !== root.id && proposed !== u.reports_to_user_id,
    };
  }

  const rootNode = buildNode(root.id, 0);

  const secondaries: ProposedTreeNode[] = [];
  for (const u of users) {
    if (u.id === root.id) continue;
    if (!proposedWires.has(u.id)) {
      const orphan = buildNode(u.id, 0);
      secondaries.push(orphan);
    }
  }
  for (const o of secondaries) {
    rootNode.children.push(o);
  }

  let maxDepth = 0;
  let totalChildren = 0;
  let parentsWithChildren = 0;
  function depth(n: ProposedTreeNode, d: number) {
    if (d > maxDepth) maxDepth = d;
    if (n.children.length > 0) {
      parentsWithChildren++;
      totalChildren += n.children.length;
    }
    for (const c of n.children) depth(c, d + 1);
  }
  depth(rootNode, 0);

  const flat: ProposedTreeNode[] = [];
  function flat_(n: ProposedTreeNode) {
    flat.push(n);
    for (const c of n.children) flat_(c);
  }
  flat_(rootNode);

  return {
    roots: [rootNode],
    flat,
    wires,
    stats: {
      totalNodes: users.length,
      newWires,
      unchangedWires,
      maxDepth,
      avgChildren: parentsWithChildren === 0 ? 0 : totalChildren / parentsWithChildren,
    },
  };
}

export async function applyAutoWire(_wires: WireEdge[]): Promise<{ applied: number }> {
  return { applied: 0 };
}