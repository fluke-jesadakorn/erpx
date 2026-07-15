import 'server-only';
import type { MessageDict, BilingualText } from '@folio-lib/i18n/types';

export function getTextServer(
  dict: MessageDict,
  key: string,
  locale?: 'th' | 'de',
): BilingualText {
  const en = dict.en[key] ?? key;
  const th = dict.th?.[key];
  const de = dict.de?.[key];
  return { en, th, de } as BilingualText;
}

export function getStringServer(
  dict: MessageDict,
  key: string,
  locale: 'th' | 'de',
): string {
  const b = getTextServer(dict, key, locale);
  return locale === 'de' ? (b.de ?? b.en) : (b.th ?? b.en);
}
