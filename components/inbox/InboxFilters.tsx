import Link from 'next/link';
import type { InboxScope } from '@/waybill/queries';
import { T } from '@/components/i18n/T';
import { Bell, Layers3, Zap, type LucideIcon } from 'lucide-react';
import { createElement } from 'react';

interface Props {
  current: InboxScope;
  counts?: Partial<Record<InboxScope, number>>;
  lang?: 'en' | 'th';
}

interface PillSpec {
  scope: InboxScope;
  href: string;
  icon: LucideIcon;
  labelId: string;
  count?: number;
}

export function InboxFilters({ current, counts, lang: _lang = 'en' }: Props) {
  const specs: Array<Omit<PillSpec, 'count'>> = [
    { scope: 'waiting',  href: '/inbox?scope=waiting',  icon: Zap, labelId: 'inbox.filterWaiting' },
    { scope: 'watching', href: '/inbox?scope=watching', icon: Bell, labelId: 'inbox.filterWatching' },
    { scope: 'all',      href: '/inbox?scope=all',      icon: Layers3, labelId: 'inbox.filterAll' },
  ];
  const pills: PillSpec[] = specs.map((p) => {
    const c = counts?.[p.scope];
    return { ...p, count: typeof c === 'number' ? c : undefined };
  });

  return (
    <nav
      aria-label="Inbox scope"
      className="glass-toolbar mb-5 flex flex-wrap gap-2 p-2 text-xs font-mono"
    >
      {pills.map((p) => {
        const isCurrent = p.scope === current;
        return (
          <Link
            key={p.scope}
            href={p.href}
            aria-current={isCurrent ? 'page' : undefined}
            className={
              'inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 transition-colors ' +
              (isCurrent
                ? 'border-info bg-info-soft text-info'
                : 'border-rule text-ink-2 hover:border-rule hover:text-ink')
            }
          >
            {createElement(p.icon, { size: 13, 'aria-hidden': true })}
            <span><T id={p.labelId} /></span>
            {typeof p.count === 'number' && (
              <span
                className={
                  'rounded-full px-1.5 py-0.5 text-xs ' +
                  (isCurrent
                    ? 'bg-info/30 text-info-strong'
                    : 'bg-paper-2 text-ink-2')
                }
              >
                {p.count}
              </span>
            )}
          </Link>
        );
      })}
    </nav>
  );
}

export default InboxFilters;
