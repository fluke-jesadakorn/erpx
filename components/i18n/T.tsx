'use client';

import React from 'react';
import { useTranslations } from 'next-intl';
import { useSecondaryLocale } from './SecondaryLocaleProvider';
import type { SecondaryLocale } from '@/i18n/config';
import type { BilingualText } from '@/i18n/types';
import thDict from '../../messages/th.json';
import deDict from '../../messages/de.json';

type Variant = 'inline' | 'compact' | 'stacked';
type Tag = 'span' | 'div' | 'p' | 'h1' | 'h2' | 'h3' | 'h4' | 'label';

interface BaseProps {
  id?: string;
  value?: BilingualText;
  values?: Record<string, string | number>;
  variant?: Variant;
  as?: Tag;
  className?: string;
  primaryClassName?: string;
  secondaryClassName?: string;
  hideSecondary?: boolean;
}

const VARIANT_SECONDARY: Record<Variant, string> = {
  inline:  'ml-1.5 text-sm font-normal text-ink-2',
  compact: 'ml-1.5 text-xs font-normal text-ink-2',
  stacked: 'block mt-0.5 text-sm font-normal text-ink-2 leading-snug',
};

const INLINE_SEP = ' · ';

const SECONDARY_DICTS: Record<SecondaryLocale, Record<string, unknown>> = {
  th: thDict as Record<string, unknown>,
  de: deDict as Record<string, unknown>,
};

function lookup(dict: Record<string, unknown>, path: string): string | undefined {
  const parts = path.split('.');
  let cur: unknown = dict;
  for (const p of parts) {
    if (cur && typeof cur === 'object' && p in (cur as Record<string, unknown>)) {
      cur = (cur as Record<string, unknown>)[p];
    } else {
      return undefined;
    }
  }
  return typeof cur === 'string' ? cur : undefined;
}

export function interpolate(text: BilingualText, values: Record<string, string | number>): BilingualText {
  const replace = (s?: string) => s?.replace(/\{(\w+)\}/g, (_, key: string) => String(values[key] ?? `{${key}}`));
  return { en: replace(text.en) ?? '', th: replace(text.th), de: replace(text.de) };
}

export function T({
  id,
  value,
  values,
  variant = 'inline',
  as,
  className,
  primaryClassName,
  secondaryClassName,
  hideSecondary,
}: BaseProps) {
  const t = useTranslations();
  const primary = value?.en ?? (id ? t(id, values as Record<string, string | number>) : '');
  const loc = useSecondaryLocale();

  const Tag = (as ?? 'span') as React.ElementType;
  const secondary =
    !hideSecondary ? value?.[loc] ?? (id ? lookup(SECONDARY_DICTS[loc], id) : undefined) : undefined;

  const basePrimaryCls =
    variant === 'stacked'
      ? 'block font-semibold text-ink leading-tight'
      : 'font-semibold text-ink';
  const pCls = primaryClassName ?? basePrimaryCls;

  if (!secondary || secondary === primary) {
    return React.createElement(
      Tag,
      { className: [className, pCls].filter(Boolean).join(' ') },
      primary,
    );
  }

  const secCls = secondaryClassName ?? VARIANT_SECONDARY[variant];

  if (variant === 'stacked') {
    return (
      <Tag className={className}>
        <span className={pCls}>{primary}</span>
        <span className={secCls} lang={loc}>
          {secondary}
        </span>
      </Tag>
    );
  }

  return (
    <Tag className={className}>
      <span className={pCls}>{primary}</span>
      <span className={secCls} lang={loc}>
        {INLINE_SEP}
        {secondary}
      </span>
    </Tag>
  );
}
