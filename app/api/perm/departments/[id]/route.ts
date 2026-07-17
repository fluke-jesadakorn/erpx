// DELETE /api/perm/departments/[id]
// Hard-delete a department: drops the user:dept:<id>::allow permission
// (cascades user_permissions) and the matching perm.roles scaffold rows
// (cascades role_permissions / department_permissions).

import { NextResponse } from 'next/server';
import { query, withTransaction } from '@/db';
import { loadActivePermSession, hasPermission, PERM, deptPermId } from '@/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function DELETE(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, PERM.rbac.matrix.edit))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id: rawId } = await ctx.params;
  const id = String(rawId ?? '').trim().toLowerCase();
  if (!/^[a-z][a-z0-9_-]{1,40}$/.test(id))
    return NextResponse.json({ error: 'invalid department id' }, { status: 400 });

  const permId = deptPermId(id);

  const summary = await withTransaction(async (q) => {
    const granted = await q<{ user_id: number }>(
      `SELECT user_id FROM perm.user_permissions
        WHERE permission_id = $1 AND revoked_at IS NULL`,
      [permId],
    );
    const grantsRevoked = await q<{ count: number }>(
      `WITH d AS (
         DELETE FROM perm.user_permissions
          WHERE permission_id = $1 AND revoked_at IS NULL
          RETURNING 1
       )
       SELECT COUNT(*)::int AS count FROM d`,
      [permId],
    );
    await q(`DELETE FROM perm.permissions WHERE id = $1`, [permId]);
    await q(`DELETE FROM perm.department_permissions WHERE department_id IN ($1, $2)`, [id, `dept-${id}`]);
    await q(`DELETE FROM perm.roles WHERE id IN ($1, $2)`, [id, `dept-${id}`]);
    return {
      affected_user_ids: granted.rows.map((r) => r.user_id),
      grants_revoked: grantsRevoked.rows[0]?.count ?? 0,
    };
  });

  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ('department.delete', $1, $2)`,
    [`user:${out.session.user.id}`, { dept_id: id, ...summary }],
  );

  return NextResponse.json({ ok: true, id, ...summary });
}
