import { NextResponse } from 'next/server';
import { query, withTransaction } from '@/db';
import { loadActivePermSession, hasPermission } from '@/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function DELETE(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, 'rbac:department:edit::allow')) {
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  }
  const { id } = await ctx.params;
  const dept = await query<{ display_name: string; is_system: boolean }>(
    `SELECT display_name, is_system FROM perm.departments WHERE id = $1`,
    [id],
  );
  if (!dept.rows[0]) return NextResponse.json({ error: 'department not found' }, { status: 404 });
  if (dept.rows[0].is_system) {
    return NextResponse.json({ error: 'Canonical departments cannot be deleted' }, { status: 403 });
  }
  const members = await query<{ count: number }>(
    `SELECT count(*)::int AS count FROM perm.user_departments WHERE department_id = $1`,
    [id],
  );
  if ((members.rows[0]?.count ?? 0) > 0) {
    return NextResponse.json({ error: 'Reassign department members before deletion' }, { status: 409 });
  }
  await withTransaction(async (q) => {
    await q(`DELETE FROM perm.departments WHERE id = $1`, [id]);
    await q(`DELETE FROM perm.permissions WHERE id = $1`, [`user:dept:${id}::allow`]);
    await q(
      `INSERT INTO perm.audit (kind, actor, target) VALUES ('department.delete', $1, $2)`,
      [`user:${out.session.user.id}`, { before: { id, ...dept.rows[0] } }],
    );
  });
  return NextResponse.json({ ok: true });
}
