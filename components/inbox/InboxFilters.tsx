import Link from 'next/link';
import type { InboxScope } from '@/waybill/queries';
import { T } from '@/components/i18n/T';

interface Props {
  current: InboxScope;
  counts?: Partial<Record<InboxScope, number>>;
  lang?: 'en' | 'th';
}

interface PillSpec {
  scope: InboxScope;
  href: string;
  icon: string;
  labelId: string;
  count?: number;
}

export function InboxFilters({ current, counts, lang: _lang = 'en' }: Props) {
  const specs: Array<Omit<PillSpec, 'count'>> = [
    { scope: 'waiting',  href: '/inbox?scope=waiting',  icon: '⚡', labelId: 'inbox.filterWaiting' },
    { scope: 'watching', href: '/inbox?scope=watching', icon: '🔔', labelId: 'inbox.filterWatching' },
    { scope: 'all',      href: '/inbox?scope=all',      icon: '🗂',  labelId: 'inbox.filterAll' },
  ];
  const pills: PillSpec[] = specs.map((p) => {
    const c = counts?.[p.scope];
    return { ...p, count: typeof c === 'number' ? c : undefined };
  });

  return (
    <nav
      aria-label="Inbox scope"
      className="mb-4 flex flex-wrap gap-2 text-xs font-mono"
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
                ? 'border-cyan-500 bg-cyan-500/15 text-cyan-200'
                : 'border-slate-700 text-slate-400 hover:border-slate-500 hover:text-slate-200')
            }
          >
            <span aria-hidden>{p.icon}</span>
            <span><T id={p.labelId} /></span>
            {typeof p.count === 'number' && (
              <span
                className={
                  'rounded-full px-1.5 py-0.5 text-xs ' +
                  (isCurrent
                    ? 'bg-cyan-500/30 text-cyan-100'
                    : 'bg-slate-800 text-slate-400')
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