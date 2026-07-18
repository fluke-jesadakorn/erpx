'use client';

import React, { useEffect, useState } from 'react';
import Link from 'next/link';
import { iconByName } from '@/components/icon';
import { subscribeBreadcrumbs, getBreadcrumbs } from '@/components/breadcrumbs/BreadcrumbSetter';
import type { Crumb } from '@/components/breadcrumbs';

export function TopbarCrumbs() {
  const [crumbs, setCrumbs] = useState<Crumb[]>(() => []);

  useEffect(() => {
    setCrumbs(getBreadcrumbs());
    return subscribeBreadcrumbs((next) => setCrumbs(next));
  }, []);

  const safe = Array.isArray(crumbs) ? crumbs.filter((c) => c && c.label) : [];

  if (safe.length === 0) return null;

  return (
    <nav aria-label="Breadcrumb" className="flex min-w-0 items-center gap-1.5 text-sm text-mute">
      {safe.map((crumb, index) => {
        const last = index === safe.length - 1;
        const IconCmp = crumb.icon ? iconByName(crumb.icon) : null;
        const content = (
          <span className={['inline-flex min-w-0 items-center gap-1.5 truncate', last ? 'text-ink' : ''].join(' ')}>
            {IconCmp && <IconCmp size={11} />}
            <span className="truncate">{crumb.label}</span>
          </span>
        );
        return (
          <React.Fragment key={`${String(crumb.label)}-${index}`}>
            {index > 0 && <span aria-hidden className="text-rule">/</span>}
            {crumb.href && !last ? (
              <Link href={crumb.href} className="hover:text-ink">{content}</Link>
            ) : (
              content
            )}
          </React.Fragment>
        );
      })}
    </nav>
  );
}

export default TopbarCrumbs;
