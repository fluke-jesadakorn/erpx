import React from 'react';
import Link from 'next/link';
import { Icon, type IconName } from '@/components/icons';

export interface Crumb {
  label: React.ReactNode;
  href?: string;
  icon?: IconName;
}

interface CrumbsProps {
  crumbs: Crumb[];
  className?: string;
}

export function Crumbs({ crumbs, className }: CrumbsProps) {
  const safe = Array.isArray(crumbs) ? crumbs.filter((c) => c && c.label) : [];
  if (safe.length === 0) return null;
  return (
    <nav aria-label="Breadcrumb" className={['flex items-center flex-wrap gap-x-1.5 gap-y-1 text-sm font-mono text-mute', className || ''].join(' ')}>
      {safe.map((c, i) => {
        const isLast = i === safe.length - 1;
        return (
          <React.Fragment key={`${String(c.label)}-${i}`}>
            {i > 0 && <span aria-hidden className="text-rule">/</span>}
            {c.href && !isLast ? (
              <Link href={c.href} className="inline-flex items-center gap-1 hover:text-ink">
                {c.icon && <Icon name={c.icon} size={10} />}
                <span>{c.label}</span>
              </Link>
            ) : (
              <span aria-current={isLast ? 'page' : undefined} className={['inline-flex items-center gap-1', isLast ? 'text-ink' : ''].join(' ')}>
                {c.icon && <Icon name={c.icon} size={10} />}
                <span>{c.label}</span>
              </span>
            )}
          </React.Fragment>
        );
      })}
    </nav>
  );
}
