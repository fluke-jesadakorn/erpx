// GET    /api/perm/users/[id]/grants  — list user's active grants + permanent perms.
// PUT    /api/perm/users/[id]/grants  — set the user's perms (mode: temporary|permanent).
//                                        body: { mode, desired_perm_ids, ends_at?, reason? }
// DELETE /api/perm/users/[id]/grants  — revoke by row id.
//                                        body: { id: number }

import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import {
  listActiveUserPerms,
  revokeUserPerm,
  setUserPermanentPerms,
} from '@/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function canMutate(s: ReturnType<typeof JSON.parse>['session'] | null) {
  if (!s) return false;
  return (
    hasPermission(s, PERM.rbac.role.assign) ||
    hasPermission(s, PERM.user.role.assign) ||
    hasPermission(s, PERM.rbac.matrix.edit) ||
    hasPermission(s, PERM.admin.system.bypass)
  );
}

function defaultEndsAt(): string {
  const d = new Date();
  d.setDate(d.getDate() + 30);
  return d.toISOString();
}

export async function GET(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!canMutate(out.session))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id: idStr } = await ctx.params;
  const userId = parseInt(idStr, 10);
  if (!userId) return NextResponse.json({ error: 'invalid id' }, { status: 400 });

  const all = await listActiveUserPerms(userId);

  return NextResponse.json({
    user_id: userId,
    grants: all,
    active_perm_ids: [
      ...new Set(
        all.filter((g) => g.revoked_at === null && (g.ends_at === null || new Date(g.ends_at).getTime() >= Date.now()))
          .map((g) => g.permission_id),
      ),
    ],
  });
}

export async function PUT(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!canMutate(out.session))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id: idStr } = await ctx.params;
  const userId = parseInt(idStr, 10);
  if (!userId) return NextResponse.json({ error: 'invalid id' }, { status: 400 });

  const body = await req.json().catch(() => ({}));
  const mode = body.mode === 'permanent' ? 'permanent' : 'temporary';
  const desired: string[] = Array.isArray(body.desired_perm_ids)
    ? body.desired_perm_ids.filter((s: unknown): s is string => typeof s === 'string')
    : [];
  const grantedBy = `user:${out.session.user.id}`;
  const reason = typeof body.reason === 'string' ? body.reason : undefined;
  const endsAt = typeof body.ends_at === 'string' ? body.ends_at : defaultEndsAt();

  const valid = await query<{ id: string }>(
    `SELECT id FROM perm.permissions WHERE id = ANY($1::text[])`,
    [desired],
  );
  const validIds = valid.rows.map((r) => r.id);

  const result = await setUserPermanentPerms({
    user_id: userId,
    desired_perm_ids: validIds,
    ends_at: mode === 'temporary' ? endsAt : null,
    granted_by: grantedBy,
    reason,
  });

  if (result.added.length || result.removed > 0) {
    await query(
      `INSERT INTO perm.audit (kind, actor, target) VALUES ($1, $2, $3)`,
      [
        mode === 'permanent' ? 'user.perm.permanent.set' : 'user.perm.temporary.set',
        grantedBy,
        {
          user_id: userId,
          mode,
          added: result.added,
          removed: result.removed,
          ends_at: mode === 'temporary' ? endsAt : null,
          reason: reason ?? null,
        },
      ],
    );
  }

  return NextResponse.json({ ok: true, mode, ...result });
}

export async function DELETE(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!canMutate(out.session))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id: idStr } = await ctx.params;
  const userId = parseInt(idStr, 10);
  if (!userId) return NextResponse.json({ error: 'invalid id' }, { status: 400 });

  const body = await req.json().catch(() => ({}));
  const id = Number(body.id);
  if (!id) return NextResponse.json({ error: 'invalid id' }, { status: 400 });

  const revokedBy = `user:${out.session.user.id}`;
  await revokeUserPerm(id, revokedBy);

  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ($1, $2, $3)`,
    [
      'user.perm.revoke',
      revokedBy,
      { user_id: userId, row_id: id },
    ],
  );

  return NextResponse.json({ ok: true });
}