'use client';

import React from 'react';
import type { ElementType, ReactNode } from 'react';
import type { SecondaryLocale } from '@folio-lib/server/locale';
import type { BilingualText } from '@folio-lib/i18n/types';
import { useSecondaryLocale } from './SecondaryLocaleProvider';

type BilingualVariant = 'inline' | 'stacked' | 'compact';

interface BilingualProps extends BilingualText {
  as?: ElementType;
  sideBySide?: boolean;
  variant?: BilingualVariant;
  showSecondary?: boolean;
  secondaryClassName?: string;
  locale?: SecondaryLocale;
  className?: string;
  children?: ReactNode;
}

const STACKED_SECONDARY =
  'block mt-0.5 text-sm font-normal text-ink-2 leading-snug';

const COMPACT_SECONDARY =
  'ml-1.5 text-xs font-normal text-mute';

const SR_ONLY =
  'sr-only';

function pickSecondaryText(text: BilingualText, locale: SecondaryLocale): string | undefined {
  const sec = text[locale];
  if (sec && sec !== text.en) return sec;
  const other = locale === 'de' ? text.th : text.de;
  if (other && other !== text.en) return other;
  if (text.th && text.th !== text.en) return text.th;
  if (text.de && text.de !== text.en) return text.de;
  return undefined;
}

export function Bilingual({
  en,
  th,
  de,
  as: Tag = 'span',
  sideBySide = true,
  variant = 'inline',
  showSecondary = true,
  secondaryClassName,
  locale,
  className,
  children,
}: BilingualProps) {
  const ctx = useSecondaryLocale();
  const active = locale ?? ctx;
  const secondaryText = pickSecondaryText({ en, th, de }, active);

  if (showSecondary === false) {
    return (
      <Tag
        className={className}
        {...(secondaryText ? { title: `${en} · ${secondaryText}` } : { title: en })}
        lang="en"
      >
        {children ?? en}
        {secondaryText ? (
          <span className={SR_ONLY} lang={active}>{secondaryText}</span>
        ) : null}
      </Tag>
    );
  }

  const secondary = active === 'de' ? de : th;
  if (!secondary || !sideBySide) {
    return <Tag className={className}>{children ?? en}</Tag>;
  }
  if (variant === 'stacked') {
    return (
      <Tag className={className}>
        <span className="block font-semibold text-ink leading-tight">{en}</span>
        <span
          className={secondaryClassName ?? STACKED_SECONDARY}
          lang="th"
        >
          {secondary}
        </span>
      </Tag>
    );
  }
  if (variant === 'compact') {
    return (
      <Tag className={className}>
        <span className="font-semibold text-ink">{en}</span>
        <span className={secondaryClassName ?? COMPACT_SECONDARY} lang="th">
          {secondary}
        </span>
      </Tag>
    );
  }
  return (
    <Tag className={className}>
      <span className="font-bold text-ink">{en}</span>
      <span
        className={
          secondaryClassName ?? 'ml-2 text-sm font-normal text-mute'
        }
        lang="th"
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

export function primaryText(
  en: string,
  _th: string | undefined,
  _de: string | undefined,
): string {
  return en;
}
