'use client';

import React from 'react';
import { Bilingual } from './Bilingual';
import type { BilingualText } from '@folio-lib/i18n/types';

interface TProps {
  value: BilingualText;
  className?: string;
  secondaryClassName?: string;
}

export function T({ value, className, secondaryClassName }: TProps) {
  return (
    <Bilingual
      en={value.en}
      th={value.th}
      de={value.de}
      className={className}
      secondaryClassName={secondaryClassName}
    />
  );
}

export function interpolate(
  text: BilingualText,
  vars: Record<string, string | number>,
): BilingualText {
  const replace = (s: string | undefined): string => {
    if (!s) return '';
    return Object.entries(vars).reduce(
      (acc, [k, v]) => acc.replaceAll(`{${k}}`, String(v)),
      s,
    );
  };
  return {
    en: replace(text.en),
    th: replace(text.th),
    de: replace(text.de),
  };
}