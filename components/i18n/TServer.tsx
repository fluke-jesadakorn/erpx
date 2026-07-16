import 'server-only';
import React from 'react';
import { getTranslations } from 'next-intl/server';
import type { SecondaryLocale } from '@/i18n/config';
import thDict from '../../messages/th.json';
import deDict from '../../messages/de.json';

type Variant = 'inline' | 'compact' | 'stacked';
type Tag = 'span' | 'div' | 'p' | 'h1' | 'h2' | 'h3' | 'h4' | 'label';

interface BaseProps {
  id: string;
  values?: Record<string, string | number>;
  variant?: Variant;
  as?: Tag;
  className?: string;
  primaryClassName?: string;
  secondaryClassName?: string;
  hideSecondary?: boolean;
  locale?: SecondaryLocale;
}

const VARIANT_SECONDARY: Record<Variant, string> = {
  inline:  'ml-1.5 text-sm font-normal text-slate-400',
  compact: 'ml-1.5 text-xs font-normal text-slate-400',
  stacked: 'block mt-0.5 text-sm font-normal text-slate-400 leading-snug',
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

export async function T({
  id,
  values,
  variant = 'inline',
  as,
  className,
  primaryClassName,
  secondaryClassName,
  hideSecondary,
  locale = 'th',
}: BaseProps) {
  const t = await getTranslations();
  const primary = t(id, values as Record<string, string | number>);
  const secondary = !hideSecondary ? lookup(SECONDARY_DICTS[locale], id) : undefined;

  const Tag = (as ?? 'span') as React.ElementType;
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
        <span className={secCls} lang={locale}>
          {secondary}
        </span>
      </Tag>
    );
  }

  return (
    <Tag className={className}>
      <span className={pCls}>{primary}</span>
      <span className={secCls} lang={locale}>
        {INLINE_SEP}
        {secondary}
      </span>
    </Tag>
  );
}
