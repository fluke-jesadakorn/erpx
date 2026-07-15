'use client';

import type { MessageDict, BilingualText } from '@folio-lib/i18n/types';

const FALLBACK: BilingualText = { en: '' };

export function useT(dict: MessageDict) {
  return (key: string): BilingualText => {
    const en = dict.en[key] ?? key;
    const th = dict.th?.[key];
    const de = dict.de?.[key];
    return { en, th, de } as BilingualText;
  };
}

export function getText(
  dict: MessageDict,
  key: string,
  _locale: 'th' | 'de',
): BilingualText {
  const en = dict.en[key] ?? key;
  const th = dict.th?.[key];
  const de = dict.de?.[key];
  return { en, th, de } as BilingualText;
}

export const EMPTY_TEXT = FALLBACK;