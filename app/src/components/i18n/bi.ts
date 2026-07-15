import type { SecondaryLocale } from '@folio-lib/server/locale';

export function bi(
  en: string,
  th?: string,
  de?: string,
  locale: SecondaryLocale = 'th',
): string {
  const sec = locale === 'de' ? de : th;
  return sec ? `${en} (${sec})` : en;
}
