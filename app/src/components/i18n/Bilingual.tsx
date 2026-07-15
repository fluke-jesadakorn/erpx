'use client';

import React from 'react';
import type { ElementType, ReactNode } from 'react';
import type { SecondaryLocale } from '@folio-lib/server/locale';
import type { BilingualText } from '@folio-lib/i18n/types';
import { useSecondaryLocale } from './SecondaryLocaleProvider';

interface BilingualProps extends BilingualText {
  as?: ElementType;
  sideBySide?: boolean;
  secondaryClassName?: string;
  locale?: SecondaryLocale;
  className?: string;
  children?: ReactNode;
}

export function Bilingual({
  en,
  th,
  de,
  as: Tag = 'span',
  sideBySide = true,
  secondaryClassName,
  locale,
  className,
  children,
}: BilingualProps) {
  const ctx = useSecondaryLocale();
  const active = locale ?? ctx;
  const secondary = active === 'de' ? de : th;
  if (!secondary || !sideBySide) {
    return <Tag className={className}>{children ?? en}</Tag>;
  }
  return (
    <Tag className={className}>
      <span className="font-bold">{en}</span>
      <span
        className={
          secondaryClassName ?? 'ml-2 text-sm opacity-70 font-normal text-slate-500'
        }
      >
        · {secondary}
      </span>
    </Tag>
  );
}

export function pickSecondary(
  text: BilingualText,
  locale: SecondaryLocale,
): string | null {
  return text[locale] ?? null;
}