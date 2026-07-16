export const PRIMARY = 'en' as const;
export type PrimaryLocale = typeof PRIMARY;

export const SECONDARIES = ['th', 'de'] as const;
export type SecondaryLocale = (typeof SECONDARIES)[number];

export function isSecondary(v: unknown): v is SecondaryLocale {
  return v === 'th' || v === 'de';
}

export const STORAGE_KEY = 'folio.lang';
export const LANG_EVENT = 'folio:lang';
export const LOCALE_COOKIE = 'folio.locale';
export const LOCALE_HEADER = 'x-folio-locale';

export function normalizeSecondaryLocale(v: unknown): SecondaryLocale {
  return isSecondary(v) ? v : 'th';
}

export const SECONDARY_BCP47: Record<SecondaryLocale, string> = {
  th: 'th-TH',
  de: 'de-DE',
};
