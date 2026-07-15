'use client';

import type { BilingualText } from '@folio-lib/i18n/types';

export const INTL_BCP47: Record<'th' | 'de', string> = {
  th: 'th-TH',
  de: 'de-DE',
};

export function intlLocale(locale: 'th' | 'de'): string {
  return INTL_BCP47[locale];
}

export function secondaryString(text: BilingualText, locale: 'th' | 'de'): string {
  return text[locale] ?? text.en;
}

export function secondaryOptional(text: BilingualText, locale: 'th' | 'de'): string | null {
  const v = text[locale];
  return v ?? null;
}
