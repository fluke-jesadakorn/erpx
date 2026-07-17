import type { BilingualText } from '@/i18n/types';

export type InterpValues = Record<string, string | number | undefined>;

const PATTERN = /\{(\w+)\}/g;

export function interpolate(
  text: string | undefined,
  values: InterpValues | undefined,
): string | undefined {
  if (text == null || !values) return text;
  return text.replace(PATTERN, (m, key: string) => {
    const v = values[key];
    return v == null ? m : String(v);
  });
}

export function interpolateBilingual(
  text: BilingualText,
  values: InterpValues,
): BilingualText {
  return {
    en: interpolate(text.en, values) ?? text.en,
    th: text.th != null ? (interpolate(text.th, values) ?? text.th) : text.th,
    de: text.de != null ? (interpolate(text.de, values) ?? text.de) : text.de,
  };
}
