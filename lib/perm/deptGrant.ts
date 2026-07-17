// lib/perm/deptGrant.ts — server-only helpers for department permission bundles.
import 'server-only';
import { query } from '../db';

export interface DeptGrant {
  department_id: string;
  permission_id: string;
  significance: boolean;
}

export async function loadDeptPermissionBundle(deptId: string): Promise<Set<string>> {
  const { rows } = await query<{ permission_id: string }>(
    `SELECT permission_id FROM perm.department_permissions WHERE department_id = $1`,
    [deptId],
  );
  return new Set(rows.map((r) => r.permission_id));
}

export async function loadDeptPermissionBundles(): Promise<Map<string, Set<string>>> {
  const { rows } = await query<{ department_id: string; permission_id: string }>(
    `SELECT department_id, permission_id FROM perm.department_permissions`,
  );
  const out = new Map<string, Set<string>>();
  for (const r of rows) {
    if (!out.has(r.department_id)) out.set(r.department_id, new Set());
    out.get(r.department_id)!.add(r.permission_id);
  }
  return out;
}

export async function loadUserDeptIds(userId: number): Promise<string[]> {
  const { rows } = await query<{ permission_id: string }>(
    `SELECT permission_id FROM perm.user_permissions
      WHERE user_id = $1
        AND permission_id LIKE 'user:dept:%::allow'
        AND revoked_at IS NULL
        AND (ends_at IS NULL OR ends_at > now())`,
    [userId],
  );
  return rows.map((r) => r.permission_id.match(/^user:dept:([^:]+)::allow$/)?.[1] ?? '');
}

export function expandUserPermissions(
  basePerms: Iterable<string>,
  deptBundles: Map<string, Set<string>>,
): Set<string> {
  const out = new Set<string>(basePerms);
  for (const perm of basePerms) {
    const m = perm.match(/^user:dept:([^:]+)::allow$/);
    if (!m) continue;
    const id = m[1];
    const bundle = deptBundles.get(id);
    if (!bundle) continue;
    for (const p of bundle) out.add(p);
  }
  return out;
}
