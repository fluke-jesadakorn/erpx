import Link from 'next/link';
import type { NotificationReadFilter, NotificationView } from '@/notifications/queries';
import { Bell, CheckCircle2, Inbox, Layers3, Zap } from 'lucide-react';

interface Props {
  current: NotificationView;
  read: NotificationReadFilter;
  domain: 'expense' | 'so' | 'all';
  counts: { actions: number; unread: number };
}

function href(view: NotificationView, read: NotificationReadFilter, domain: string): string {
  const params = new URLSearchParams({ view, read, domain });
  return `/inbox?${params.toString()}`;
}

export function InboxFilters({ current, read, domain, counts }: Props) {
  const tabs = [
    { value: 'actions' as const, label: 'Action required', icon: Zap, count: counts.actions },
    { value: 'notifications' as const, label: 'Notifications', icon: Bell, count: counts.unread },
    { value: 'all' as const, label: 'All', icon: Layers3, count: undefined },
  ];
  return (
    <div className="glass-toolbar mb-5 space-y-3 p-2 text-xs font-mono">
      <nav aria-label="Inbox type" className="flex flex-wrap gap-2">
        {tabs.map((tab) => {
          const Icon = tab.icon;
          const active = current === tab.value;
          return (
            <Link key={tab.value} href={href(tab.value, read, domain)} className={`inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 transition-colors ${active ? 'border-info bg-info-soft text-info' : 'border-rule text-ink-2 hover:text-ink'}`}>
              <Icon size={13} aria-hidden />
              <span>{tab.label}</span>
              {tab.count != null && <span className="rounded-full bg-paper-2 px-1.5 py-0.5">{tab.count}</span>}
            </Link>
          );
        })}
      </nav>
      <div className="flex flex-wrap items-center gap-2 border-t border-rule pt-2">
        <span className="text-mute">Read:</span>
        {(['all', 'unread', 'read'] as const).map((value) => (
          <Link key={value} href={href(current, value, domain)} className={`inline-flex items-center gap-1 rounded-md px-2 py-1 ${read === value ? 'bg-paper-3 text-ink' : 'text-ink-2 hover:text-ink'}`}>
            {value === 'unread' ? <Inbox size={12} /> : value === 'read' ? <CheckCircle2 size={12} /> : null}
            {value}
          </Link>
        ))}
        <span className="ml-2 text-mute">Domain:</span>
        {(['all', 'expense', 'so'] as const).map((value) => (
          <Link key={value} href={href(current, read, value)} className={`rounded-md px-2 py-1 ${domain === value ? 'bg-paper-3 text-ink' : 'text-ink-2 hover:text-ink'}`}>
            {value === 'so' ? 'Sales orders' : value === 'expense' ? 'Expenses' : 'All'}
          </Link>
        ))}
      </div>
    </div>
  );
}
