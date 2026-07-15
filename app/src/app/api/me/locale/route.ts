import { NextResponse } from 'next/server';
import { cookies } from 'next/headers';
import { query } from '@folio-lib/db';
import { verifySession, SESSION_COOKIE } from '@folio-lib/server/sessionToken';
import { isSecondaryLocale } from '@folio-lib/server/locale';

export async function POST(req: Request) {
  let body: unknown;
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ ok: false, error: 'invalid_json' }, { status: 400 });
  }
  const locale = (body as { locale?: unknown })?.locale;
  if (!isSecondaryLocale(locale)) {
    return NextResponse.json({ ok: false, error: 'invalid_locale' }, { status: 400 });
  }

  const token = req.headers
    .get('cookie')
    ?.match(new RegExp(`(?:^|;\\s*)${SESSION_COOKIE}=([^;]+)`))?.[1];
  const payload = await verifySession(token ? decodeURIComponent(token) : null);
  if (!payload) {
    return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });
  }

  await query(`UPDATE users SET secondary_locale = $1 WHERE id = $2`, [
    locale,
    payload.sub,
  ]);
  await query(`UPDATE auth.sessions SET locale = $1 WHERE id = $2`, [
    locale,
    payload.id,
  ]);

  const c = await cookies();
  c.set('folio.locale', locale, {
    path: '/',
    maxAge: 60 * 60 * 24 * 365,
    sameSite: 'lax',
  });
  return NextResponse.json({ ok: true, locale });
}