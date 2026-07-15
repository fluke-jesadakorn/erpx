import 'server-only';
import { cookies } from 'next/headers';
import { query } from '../db';
import { verifySession } from './sessionToken';

export type SecondaryLocale = 'th' | 'de';

const COOKIE_KEY = 'folio.locale';
const HEADER_KEY = 'x-folio-locale';

export function isSecondaryLocale(v: unknown): v is SecondaryLocale {
  return v === 'th' || v === 'de';
}

export function normalizeSecondaryLocale(v: unknown): SecondaryLocale {
  return v === 'de' ? 'de' : 'th';
}

export async function getSecondaryLocale(): Promise<SecondaryLocale> {
  try {
    const c = await cookies();
    const cookieVal = c.get(COOKIE_KEY)?.value;
    if (isSecondaryLocale(cookieVal)) return cookieVal;

    const token = c.get('folio_session')?.value ?? null;
    const payload = await verifySession(token);
    if (payload) {
      const r = await query<{ locale: string }>(
        `SELECT locale FROM auth.sessions WHERE id = $1 LIMIT 1`,
        [payload.id],
      );
      const v = r.rows[0]?.locale;
      if (isSecondaryLocale(v)) return v;
    }

    const erpTok = c.get('folio_session')?.value ?? null;
    const pl = await verifySession(erpTok);
    if (pl) {
      const r = await query<{ secondary_locale: string }>(
        `SELECT secondary_locale FROM users WHERE id = $1 LIMIT 1`,
        [pl.sub],
      );
      return normalizeSecondaryLocale(r.rows[0]?.secondary_locale);
    }
  } catch {
  }
  return 'th';
}

export function getSecondaryLocaleFromHeaders(
  headers: Record<string, string | string[] | undefined> | Headers,
): SecondaryLocale {
  const get = (name: string): string | undefined => {
    if (headers && typeof (headers as Headers).get === 'function') {
      return (headers as Headers).get(name) ?? undefined;
    }
    const v = (headers as Record<string, string | string[] | undefined>)[name];
    if (typeof v === 'string') return v;
    if (Array.isArray(v) && v.length > 0 && typeof v[0] === 'string') return v[0];
    return undefined;
  };
  const h = get(HEADER_KEY);
  if (isSecondaryLocale(h)) return h;
  const cookie = get('cookie');
  if (cookie) {
    const match = cookie.match(/(?:^|;\s*)folio\.locale=([^;]+)/);
    if (match) {
      const v = decodeURIComponent(match[1]);
      if (isSecondaryLocale(v)) return v;
    }
  }
  return 'th';
}

export const LOCALE_COOKIE = COOKIE_KEY;
export const LOCALE_HEADER = HEADER_KEY;