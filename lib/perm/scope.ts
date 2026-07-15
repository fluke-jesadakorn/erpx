// lib/perm/scope.ts — derive actor visibility scope from session permissions.
//
// Scope kinds:
//   - 'all'        : admin / system bypass
//   - 'department' : same-dept only
//   - 'subtree'    : self + all reports-to descendants
//   - 'self'       : just self
//
// Department comes from the permission list (`user:dept:<id>::allow`).

import 'server-only';
import { query } from '../db';
import { SYSTEM_PERMS, parseDeptFromPerms, parseDeptsFromPerms } from './grammar';

export type ScopeKind = 'self' | 'department' | 'all' | 'subtree';

export interface ActorScope {
  kind: ScopeKind;
  userId: number;
  deptId: string | null;
  subtreeUserIds: number[];
}

function qualifierOf(perm: string): string | null {
  const idx = perm.indexOf('::');
  if (idx < 0) return null;
  const head = perm.slice(0, idx);
  const seg = head.split(':');
  return seg.length === 4 ? seg[3] : null;
}

export async function getActorScope(
  permSet: Set<string>,
  userId: number,
): Promise<ActorScope> {
  const deptId = parseDeptFromPerms(permSet);

  let hasAll = false;
  let hasSubtree = false;
  let hasDept = false;

  for (const perm of permSet) {
    if (SYSTEM_PERMS.has(perm)) {
      hasAll = true;
      break;
    }
    const q = qualifierOf(perm);
    if (q === 'all' || q === '*') hasAll = true;
    else if (q === 'subtree') hasSubtree = true;
    else if (q === 'dept') hasDept = true;
  }

  let kind: ScopeKind;
  if (hasAll) kind = 'all';
  else if (hasSubtree) kind = 'subtree';
  else if (hasDept) kind = 'department';
  else kind = 'self';

  let subtreeUserIds: number[] = [];
  if (kind === 'subtree') {
    subtreeUserIds = await computeSubtree(userId);
  }

  return { kind, userId, deptId, subtreeUserIds };
}

async function computeSubtree(userId: number): Promise<number[]> {
  const r = await query<{ id: number }>(
    `WITH RECURSIVE down AS (
       SELECT id, reports_to_user_id FROM users WHERE id = $1
       UNION
       SELECT u.id, u.reports_to_user_id
         FROM users u JOIN down d ON u.reports_to_user_id = d.id
     )
     SELECT id FROM down WHERE id <> $1`,
    [userId],
  );
  return r.rows.map((row) => row.id);
}

export interface ScopeFilter {
  clause: string;
  params: unknown[];
}

export function scopeFilter(
  scope: ActorScope,
  userColumn: string = 'submitter_id',
): ScopeFilter {
  switch (scope.kind) {
    case 'all':
      return { clause: '', params: [] };
    case 'department':
      if (!scope.deptId) {
        return { clause: `${userColumn} = $1`, params: [scope.userId] };
      }
      return { clause: `${userColumn} = ANY($1::int[])`, params: [usersInDept(scope.deptId) as unknown as number[]] };
    case 'subtree':
      if (!scope.subtreeUserIds.length) {
        return { clause: `${userColumn} = $1`, params: [scope.userId] };
      }
      return { clause: `${userColumn} = ANY($1)`, params: [scope.subtreeUserIds] };
    case 'self':
    default:
      return { clause: `${userColumn} = $1`, params: [scope.userId] };
  }
}

// Placeholder resolver — the caller is expected to pass resolved ids when needed.
// For 'department' scope without pre-resolved ids, fall back to self-only.
function usersInDept(_deptId: string): number[] {
  return [];
}

export async function assertInScope(
  scope: ActorScope,
  targetUserId: number,
): Promise<void> {
  if (scope.kind === 'all') return;
  if (scope.kind === 'self') {
    if (targetUserId !== scope.userId) {
      throw new Error('Permission denied: not in scope');
    }
    return;
  }
  if (scope.kind === 'subtree') {
    if (scope.subtreeUserIds.includes(targetUserId)) return;
    throw new Error('Permission denied: target not in subtree');
  }
  if (scope.kind === 'department') {
    if (!scope.deptId) {
      throw new Error('Permission denied: department scope requires dept binding');
    }
    const { rows } = await query<{ user_id: number }>(
      `SELECT ur.user_id FROM perm.user_permissions ur
        WHERE ur.permission_id = $1 AND ur.revoked_at IS NULL`,
      [`user:dept:${scope.deptId}::allow`],
    );
    if (rows.some((r) => r.user_id === targetUserId)) return;
    throw new Error('Permission denied: target not in department');
  }
}

export { parseDeptFromPerms, parseDeptsFromPerms };
