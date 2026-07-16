import React from 'react';
import Link from 'next/link';

export interface CategoryChip {
  label: React.ReactNode;
  icon: string;
  href?: string;
}

interface PageLayoutProps {
  title?: React.ReactNode;
  subtitle?: React.ReactNode;
  category?: CategoryChip;
  actions?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}

export const PageLayout: React.FC<PageLayoutProps> = ({
  title,
  subtitle,
  category,
  actions,
  children,
  className,
}) => {
  return (
    <main
      className={[
        'max-w-6xl mx-auto px-6 py-10 text-slate-100 selection:bg-indigo-500 selection:text-white',
        className || '',
      ].join(' ')}
    >
      <div className="flex flex-wrap items-center gap-2 mb-1">
        {category && (
          <Link
            href={category.href ?? '#'}
            className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full border border-slate-800 bg-slate-900/60 text-sm font-mono text-slate-400 hover:text-white hover:border-slate-700 transition-colors"
          >
            <span aria-hidden>{category.icon}</span>
            <span>{category.label}</span>
          </Link>
        )}
      </div>

      {(title || actions) && (
        <div className="flex flex-wrap items-start justify-between gap-3">
          {(title || subtitle) && (
            <div>
              {title && (
                <h1 className="text-[28px] font-bold tracking-tight text-white leading-tight">
                  {title}
                </h1>
              )}

              {subtitle && (
                <p className="mt-1 text-[13px] text-slate-400 leading-relaxed">
                  {subtitle}
                </p>
              )}
            </div>
          )}
          {actions && <div className="flex flex-wrap gap-2">{actions}</div>}
        </div>
      )}

      <div className="mt-8">{children}</div>
    </main>
  );
};

export default PageLayout;