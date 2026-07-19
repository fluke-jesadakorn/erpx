'use server';

import { cookies } from 'next/headers';
import { redirect } from 'next/navigation';
import { revalidateTag } from 'next/cache';
import { query } from '@/db';
import {
  SESSION_COOKIE,
  SESSION_TTL_SECONDS,
  safeEqual,
  signSession,
  verifySession,
  mintSessionId,
} from '@/server/sessionToken';

function prod(): boolean {
  return process.env.NODE_ENV === 'production';
}

export async function switchActor(formData: FormData): Promise<{ ok: boolean; user?: { id: number; name: string; role: string } }> {
  if (prod()) throw new Error('disabled in production');

  const id = parseInt(String(formData.get('id') ?? ''), 10);
  if (!id) throw new Error('id required');

  const expectedPin = process.env.DEV_ACTOR_PIN;
  if (expectedPin) {
    const provided = String(formData.get('pin') ?? '');
    if (!safeEqual(provided, expectedPin)) throw new Error('invalid pin');
  }

  const r = await query<{ id: number; fullname: string; role_id: string | null }>(
    `SELECT u.id, u.fullname,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
              LIMIT 1) AS role_id
       FROM users u
      WHERE u.id = $1 AND u.is_active IS TRUE`,
    [id],
  );
  if (!r.rows.length) throw new Error('user not found');

  const roleName = r.rows[0].role_id ?? 'unconfigured';

  const c = await cookies();
  const prevTok = c.get(SESSION_COOKIE)?.value ?? null;
  const prevPayload = await verifySession(prevTok);

  const sid = mintSessionId();
  await query(
    `INSERT INTO auth.sessions (id, user_id, role, expires_at)
     VALUES ($1, $2, $3, now() + ($4 || ' seconds')::interval)`,
    [sid, id, roleName, SESSION_TTL_SECONDS],
  );

  if (prevPayload && prevPayload.id !== sid) {
    await query(`UPDATE auth.sessions SET revoked_at = now() WHERE id = $1`, [prevPayload.id]).catch(() => {});
  }

  const token = await signSession({ id: sid, sub: id, role: roleName, impersonatorUserId: null });

  c.set(SESSION_COOKIE, token, {
    httpOnly: true,
    sameSite: 'lax',
    secure: process.env.NODE_ENV === 'production',
    path: '/',
    maxAge: 60 * 60 * 24 * 30,
  });

  revalidateTag('actor', 'max');

  return {
    ok: true,
    user: { id: r.rows[0].id, name: r.rows[0].fullname, role: roleName },
  };
}

export async function signOutActor(): Promise<void> {
  if (prod()) throw new Error('disabled in production');
  const c = await cookies();
  const tok = c.get(SESSION_COOKIE)?.value ?? null;
  const payload = await verifySession(tok);
  if (payload) {
    await query(`UPDATE auth.sessions SET revoked_at = now() WHERE id = $1`, [payload.id]).catch(() => {});
  }
  c.delete(SESSION_COOKIE);
  revalidateTag('actor', 'max');
  redirect('/login');
}
