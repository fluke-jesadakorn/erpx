// POST /api/actor  { id, pin? } — set the signed actor cookie.
// In production this route should be disabled entirely (real auth would set the cookie).
// In dev/non-prod, a DEV_ACTOR_PIN (if configured) must be supplied.

import { NextResponse } from 'next/server';
import { revalidatePath } from 'next/cache';
import { cookies } from 'next/headers';
import { query } from '@/db';
import { signSession, SESSION_COOKIE, SESSION_TTL_SECONDS, mintSessionId } from '@/server/sessionToken';
import { safeEqual } from '@/server/sessionToken';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function prod(): boolean {
  return process.env.NODE_ENV === 'production';
}

export async function POST(req: Request) {
  if (prod()) {
    return NextResponse.json({ error: 'disabled in production' }, { status: 404 });
  }

  const body = await req.json().catch(() => ({}));
  const id = parseInt(body.id, 10);
  if (!id) return NextResponse.json({ error: 'id required' }, { status: 400 });

  const expectedPin = process.env.DEV_ACTOR_PIN;
  if (expectedPin) {
    const provided = String(body.pin || '');
    if (!safeEqual(provided, expectedPin)) {
      return NextResponse.json({ error: 'invalid pin' }, { status: 401 });
    }
  }

  const r = await query('SELECT id, fullname FROM users WHERE id = $1', [id]);
  if (!r.rows.length) return NextResponse.json({ error: 'user not found' }, { status: 404 });

  const roleRow = await query<{ id: string; name: string }>(
    `SELECT pr.id, split_part(pr.id, '::', 1) AS name FROM perm.user_roles ur
       JOIN perm.roles pr ON pr.id = ur.role_id
      WHERE ur.user_id = $1
      ORDER BY (CASE WHEN pr.id LIKE '%::1' THEN 0
                     WHEN pr.id LIKE '%::2' THEN 1
                     WHEN pr.id LIKE '%::3' THEN 2
                     WHEN pr.id LIKE '%::4' THEN 3
                     WHEN pr.id LIKE '%::5' THEN 4
                     ELSE 5 END), ur.granted_at ASC
      LIMIT 1`,
    [id],
  );
  const roleName = roleRow.rows[0]?.name || 'officer';

  const sid = mintSessionId();
  await query(
    `INSERT INTO auth.sessions (id, user_id, role, expires_at)
     VALUES ($1, $2, $3, now() + ($4 || ' seconds')::interval)`,
    [sid, id, roleName, SESSION_TTL_SECONDS],
  );

  const token = await signSession({
    id: sid,
    sub: id,
    role: roleName,
    impersonatorUserId: null,
  });

  const c = await cookies();
  c.set(SESSION_COOKIE, token, {
    httpOnly: true,
    sameSite: 'lax',
    secure: process.env.NODE_ENV === 'production',
    path: '/',
    maxAge: 60 * 60 * 24 * 30,
  });

  revalidatePath('/', 'layout');

  return NextResponse.json({ ok: true, user: r.rows[0], role: roleName });
}

export async function DELETE() {
  if (prod()) {
    return NextResponse.json({ error: 'disabled in production' }, { status: 404 });
  }
  const c = await cookies();
  c.delete(SESSION_COOKIE);
  return NextResponse.json({ ok: true });
}