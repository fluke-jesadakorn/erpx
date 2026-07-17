// POST /api/policy/targets — create a new department or specific role target.
// GET  /api/policy/full   — full matrix data (columns, targets, grants).
import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActivePermSession, hasPermission, PERM, buildPerm } from '@/perm/server';
import {
  loadMatrixColumns,
  loadMatrixTargets,
  loadMatrixCells,
} from '@/policy/matrixRepo';
import 'server-only';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: Request) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, PERM.rbac.matrix.edit))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const body = await req.json().catch(() => ({}));
  const kind: 'department' | 'role' = body.kind === 'department' ? 'department' : body.kind === 'role' ? 'role' : 'department';
  const idRaw = String(body.id ?? '').trim().toLowerCase();
  const label = String(body.label ?? '').trim();

  if (!/^[a-z][a-z0-9_-]{1,40}$/.test(idRaw))
    return NextResponse.json({ error: 'id must be snake/kebab case (a-z, 0-9, _, -)' }, { status: 400 });

  const actor = `user:${out.session.user.id}`;

  if (kind === 'department') {
    const permId = buildPerm({ domain: 'user', subject: 'dept', verb: idRaw });
    const exists = await query<{ id: string }>(`SELECT id FROM perm.permissions WHERE id = $1`, [permId]);
    if (exists.rows.length === 0) {
      await query(
        `INSERT INTO perm.permissions (id, description) VALUES ($1, $2)`,
        [permId, `Department membership: ${label || idRaw}`],
      );
    }
    const roleExists = await query<{ id: string }>(`SELECT id FROM perm.roles WHERE id = $1`, [idRaw]);
    if (roleExists.rows.length === 0) {
      await query(
        `INSERT INTO perm.roles (id, display_name, description, is_system, sort_order)
         VALUES ($1, $2, $3, false, 50)`,
        [idRaw, label || idRaw, 'Department target'],
      );
    }
    await query(
      `INSERT INTO perm.audit (kind, actor, target) VALUES ('policy.target.create', $1, $2)`,
      [actor, { kind, id: idRaw, label }],
    );
    return NextResponse.json({ ok: true, id: idRaw, kind, perm_id: permId });
  }

  // role
  const level = Number(body.level);
  if (!Number.isFinite(level) || level < 1 || level > 10)
    return NextResponse.json({ error: 'level must be 1-10' }, { status: 400 });
  const roleId = `${idRaw}::${Math.floor(level)}`;
  const dup = await query<{ id: string }>(`SELECT id FROM perm.roles WHERE id = $1`, [roleId]);
  if (dup.rows.length > 0)
    return NextResponse.json({ error: `Role "${roleId}" already exists` }, { status: 409 });
  await query(
    `INSERT INTO perm.roles (id, display_name, description, is_system, sort_order)
     VALUES ($1, $2, $3, false, 200)`,
    [roleId, label || roleId, 'Specific role target'],
  );
  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ('policy.target.create', $1, $2)`,
    [actor, { kind, id: roleId, label }],
  );
  return NextResponse.json({ ok: true, id: roleId, kind });
}

export async function GET(req: Request) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, PERM.rbac.matrix.view))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const [columns, targets, cells] = await Promise.all([
    loadMatrixColumns(),
    loadMatrixTargets(),
    loadMatrixCells(),
  ]);
  const grants = columns.map((c) => ({ perm: c.perm }));
  const cellsObj: Record<string, string[]> = {};
  for (const [tid, set] of cells) cellsObj[tid] = Array.from(set);
  return NextResponse.json({ columns, targets, grants, cells: cellsObj });
}
