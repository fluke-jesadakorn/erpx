import 'server-only';
import { query, withTransaction } from '../db';
import { buildPerm } from './grammar';

export function deptPermId(deptId: string): string {
  return buildPerm({ domain: 'user', subject: 'dept', verb: deptId });
}

export interface DeptMutation {
  dept_id: string;
  added?: string[];
  removed?: number;
  cleared?: number;
}

export async function setUserDept(userId: number, deptId: string, grantedBy: string): Promise<DeptMutation> {
  const newPerm = deptPermId(deptId);
  return withTransaction(async (q) => {
    const existing = await q<{ id: number; permission_id: string }>(
      `SELECT id, permission_id
         FROM perm.user_permissions
        WHERE user_id = $1
          AND permission_id LIKE 'user:dept:%::allow'
          AND revoked_at IS NULL
        FOR UPDATE`,
      [userId],
    );
    const removed = existing.rows.length;
    if (removed > 0) {
      await q(
        `UPDATE perm.user_permissions
            SET revoked_at = now(), revoked_by = $2
          WHERE user_id = $1
            AND permission_id LIKE 'user:dept:%::allow'
            AND revoked_at IS NULL`,
        [userId, grantedBy],
      );
    }
    await q(
      `INSERT INTO perm.user_permissions (user_id, permission_id, granted_by)
       VALUES ($1, $2, $3)`,
      [userId, newPerm, grantedBy],
    );
    return { dept_id: deptId, added: [newPerm], removed, cleared: removed };
  });
}

export async function clearUserDept(userId: number, revokedBy: string): Promise<number> {
  const r = await query(
    `UPDATE perm.user_permissions
        SET revoked_at = now(), revoked_by = $2
      WHERE user_id = $1
        AND permission_id LIKE 'user:dept:%::allow'
        AND revoked_at IS NULL`,
    [userId, revokedBy],
  );
  return r.rowCount ?? 0;
}
