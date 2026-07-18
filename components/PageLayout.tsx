import React from 'react';
import Link from 'next/link';
import { iconByName, type IconName } from '@/components/icon';

export interface CategoryChip {
  label: React.ReactNode;
  icon: IconName;
  href?: string;
}

interface PageLayoutProps {
  title?: React.ReactNode;
  subtitle?: React.ReactNode;
  category?: CategoryChip;
  actions?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
  contentClassName?: string;
  density?: 'comfortable' | 'compact';
  width?: 'standard' | 'wide' | 'full';
}

export const PageLayout: React.FC<PageLayoutProps> = ({
  title,
  subtitle,
  category,
  actions,
  children,
  className,
  contentClassName,
  density = 'comfortable',
  width = 'standard',
}) => {
  const densityCls =
    density === 'compact'
      ? 'px-3 py-2'
      : 'px-[var(--page-pad-x)] py-[var(--page-pad-y)]';
  const widthCls =
    width === 'wide'
      ? 'max-w-[var(--page-wide-w)]'
      : width === 'full'
        ? 'max-w-none'
        : 'max-w-[var(--page-max-w)]';

  return (
    <div
      className={[
        'mx-auto w-full text-ink selection:bg-accent selection:text-ink',
        widthCls,
        densityCls,
        className || '',
      ].join(' ')}
    >
      {(category || title || subtitle || actions) && (
      <header className="panel-elevated relative overflow-hidden px-5 py-5 sm:px-6 sm:py-6">
        <span aria-hidden className="absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-accent/65 to-transparent" />
      <div className="mb-2 flex flex-wrap items-center gap-2">
        {category && (() => {
          const IconCmp = iconByName(category.icon);
          return (
            <Link
              href={category.href ?? '#'}
              className="glass-chip inline-flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium uppercase tracking-[0.12em] text-ink-2 transition-colors hover:border-rule-strong hover:text-ink"
            >
              <IconCmp size={12} aria-hidden />
              <span>{category.label}</span>
            </Link>
          );
        })()}
      </div>

      {(title || subtitle || actions) && (
        <div className="flex flex-wrap items-start justify-between gap-4">
          {(title || subtitle) && (
            <div>
              {title && (
                <h1 className="page-title text-ink">
                  {title}
                </h1>
              )}

              {subtitle && (
                <p className="mt-2 max-w-3xl text-sm leading-relaxed text-ink-2">
                  {subtitle}
                </p>
              )}
            </div>
          )}
          {actions && <div className="flex flex-wrap gap-2">{actions}</div>}
        </div>
      )}
      </header>
      )}

      <div className={[(category || title || subtitle || actions) ? 'mt-6' : '', contentClassName].filter(Boolean).join(' ')}>{children}</div>
    </div>
  );
};

export default PageLayout;
