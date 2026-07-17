// PUT /api/policy/grants — replace a target's allow grants (role OR department).
import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActivePermSession, hasPermission, PERM, effectOf } from '@/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface Entry {
  target_kind: 'department' | 'role';
  target_id: string;
  allow: string[];
  significance?: Record<string, boolean>;
}

export async function PUT(req: Request) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, PERM.rbac.matrix.edit))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const body = await req.json().catch(() => ({}));
  const entries: Entry[] = Array.isArray(body.entries)
    ? body.entries.filter((e: any) =>
        e && typeof e.target_id === 'string' &&
        (e.target_kind === 'department' || e.target_kind === 'role') &&
        Array.isArray(e.allow))
    : [];
  if (!entries.length)
    return NextResponse.json({ error: 'entries[] required' }, { status: 400 });

  const actor = `user:${out.session.user.id}`;
  const summary: { kind: string; id: string; added: number; removed: number }[] = [];

  for (const e of entries) {
    const desired = e.allow.filter((p: string) => effectOf(p) !== 'deny');
    const table = e.target_kind === 'department' ? 'perm.department_permissions' : 'perm.role_permissions';
    const keyCol = e.target_kind === 'department' ? 'department_id' : 'role_id';
    const existing = await query<{ permission_id: string }>(
      `SELECT permission_id FROM ${table} WHERE ${keyCol} = $1`,
      [e.target_id],
    );
    const before = new Set(existing.rows.map((r) => r.permission_id));
    const after = new Set(desired);
    const removed = [...before].filter((p) => !after.has(p));
    const added = [...after].filter((p) => !before.has(p));

    if (removed.length) {
      await query(
        `DELETE FROM ${table} WHERE ${keyCol} = $1 AND permission_id = ANY($2::text[])`,
        [e.target_id, removed],
      );
    }
    for (const p of added) {
      const sig = e.significance && Object.prototype.hasOwnProperty.call(e.significance, p)
        ? !!e.significance[p]
        : e.target_kind === 'department';
      await query(
        `INSERT INTO ${table} (${keyCol}, permission_id, significance, granted_by)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT DO NOTHING`,
        [e.target_id, p, sig, actor],
      );
    }
    summary.push({ kind: e.target_kind, id: e.target_id, added: added.length, removed: removed.length });

    if (added.length || removed.length) {
      await query(
        `INSERT INTO perm.audit (kind, actor, target) VALUES ('policy.grants.sync', $1, $2)`,
        [actor, { kind: e.target_kind, id: e.target_id, added, removed }],
      );
    }
  }

  return NextResponse.json({ ok: true, summary });
}
