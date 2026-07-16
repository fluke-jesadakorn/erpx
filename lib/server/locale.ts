import 'server-only';
import { cookies } from 'next/headers';
import { isSecondary, normalizeSecondaryLocale, LOCALE_HEADER, LOCALE_COOKIE, type SecondaryLocale } from '@/i18n/config';
import { query } from '../db';
import { verifySession } from './sessionToken';

export type { SecondaryLocale };
export { isSecondary, isSecondary as isSecondaryLocale, normalizeSecondaryLocale, LOCALE_COOKIE, LOCALE_HEADER };

export async function getSecondaryLocale(): Promise<SecondaryLocale> {
  try {
    const c = await cookies();
    const cookieVal = c.get(LOCALE_COOKIE)?.value;
    if (isSecondary(cookieVal)) return cookieVal;

    const token = c.get('folio_session')?.value ?? null;
    const payload = await verifySession(token);
    if (payload) {
      const r = await query<{ locale: string }>(
        `SELECT locale FROM auth.sessions WHERE id = $1 LIMIT 1`,
        [payload.id],
      );
      const v = r.rows[0]?.locale;
      if (isSecondary(v)) return v;

      const erpTok = c.get('folio_session')?.value ?? null;
      const pl = await verifySession(erpTok);
      if (pl) {
        const r2 = await query<{ secondary_locale: string }>(
          `SELECT secondary_locale FROM users WHERE id = $1 LIMIT 1`,
          [pl.sub],
        );
        return normalizeSecondaryLocale(r2.rows[0]?.secondary_locale);
      }
    }
  } catch {}
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
  const h = get(LOCALE_HEADER);
  if (isSecondary(h)) return h;
  const cookie = get('cookie');
  if (cookie) {
    const match = cookie.match(/(?:^|;\s*)folio\.locale=([^;]+)/);
    if (match) {
      const v = decodeURIComponent(match[1]);
      if (isSecondary(v)) return v;
    }
  }
  return 'th';
}
