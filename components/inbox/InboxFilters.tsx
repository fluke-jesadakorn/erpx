import Link from 'next/link';
import type { NotificationReadFilter, NotificationView } from '@/notifications/queries';
import { Bell, CheckCheck, Inbox, Layers3, Zap } from 'lucide-react';

interface Props {
  current: NotificationView;
  read: NotificationReadFilter;
  domain: 'expense' | 'so' | 'pr' | 'po' | 'all';
  counts: { actions: number; unread: number };
}

function href(view: NotificationView, read: NotificationReadFilter, domain: string): string {
  const params = new URLSearchParams({ view, read, domain });
  return `/inbox?${params.toString()}`;
}

export function InboxFilters({ current, read, domain, counts }: Props) {
  const tabs = [
    { value: 'actions' as const,       label: 'Action required', icon: Zap,     count: counts.actions, tone: 'caution' },
    { value: 'notifications' as const, label: 'Notifications',   icon: Bell,    count: counts.unread,  tone: 'info' },
    { value: 'all' as const,           label: 'All',             icon: Layers3, count: undefined,      tone: 'neutral' },
  ];

  return (
    <div className="panel-floating mb-5 space-y-3 p-2.5">
      <nav aria-label="Inbox type" className="flex flex-wrap gap-1.5">
        {tabs.map((tab) => {
          const Icon = tab.icon;
          const active = current === tab.value;
          return (
            <Link
              key={tab.value}
              href={href(tab.value, read, domain)}
              className={[
                'group inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium transition-all',
                active
                  ? 'border-accent/50 bg-accent-soft/70 text-accent shadow-sm'
                  : 'border-rule bg-paper-2/50 text-ink-2 hover:border-rule-strong hover:bg-paper-3/70 hover:text-ink',
              ].join(' ')}
            >
              <Icon size={13} aria-hidden />
              <span>{tab.label}</span>
              {tab.count != null && tab.count > 0 && (
                <span
                  className={[
                    'inline-flex h-4 min-w-[16px] items-center justify-center rounded-full px-1 text-[10px] font-mono font-semibold',
                    active
                      ? 'bg-accent text-paper'
                      : tab.tone === 'caution'
                        ? 'bg-caution-soft/70 text-caution'
                        : tab.tone === 'info'
                          ? 'bg-info-soft/70 text-info'
                          : 'bg-paper-3 text-ink-2',
                  ].join(' ')}
                >
                  {tab.count > 99 ? '99+' : tab.count}
                </span>
              )}
            </Link>
          );
        })}
      </nav>

      <div className="flex flex-wrap items-center gap-2 border-t border-rule/60 pt-2.5 text-xs">
        <span className="font-mono text-[10px] uppercase tracking-wider text-mute">Read</span>
        <div className="inline-flex items-center gap-0.5 rounded-lg border border-rule bg-paper-2/60 p-0.5">
          {(['all', 'unread', 'read'] as const).map((value) => {
            const active = read === value;
            const Icon = value === 'unread' ? Inbox : value === 'read' ? CheckCheck : Layers3;
            return (
              <Link
                key={value}
                href={href(current, value, domain)}
                className={[
                  'inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 font-mono uppercase tracking-wide transition-colors',
                  active ? 'bg-accent text-paper' : 'text-ink-2 hover:text-ink',
                ].join(' ')}
              >
                <Icon size={11} aria-hidden />
                <span>{value}</span>
              </Link>
            );
          })}
        </div>

        <span className="ml-2 font-mono text-[10px] uppercase tracking-wider text-mute">Domain</span>
        <div className="inline-flex items-center gap-0.5 rounded-lg border border-rule bg-paper-2/60 p-0.5">
          {(['all', 'expense', 'pr', 'po', 'so'] as const).map((value) => {
            const active = domain === value;
            const label = value === 'so' ? 'Sales' : value === 'po' ? 'Purchase orders' : value === 'pr' ? 'Purchase requests' : value === 'expense' ? 'Expense' : 'All';
            return (
              <Link
                key={value}
                href={href(current, read, value)}
                className={[
                  'rounded-md px-2.5 py-1 font-mono uppercase tracking-wide transition-colors',
                  active ? 'bg-accent text-paper' : 'text-ink-2 hover:text-ink',
                ].join(' ')}
              >
                {label}
              </Link>
            );
          })}
        </div>
      </div>
    </div>
  );
}
