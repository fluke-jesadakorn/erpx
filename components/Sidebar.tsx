'use client';

import React from 'react';
import Link from 'next/link';
import { usePathname, useSearchParams } from 'next/navigation';
import { Icon, type IconName } from '@/components/icons';
import { matchSidebar, SIDEBAR_GROUPS, type SidebarLabel } from './sidebar-config';
import { T } from '@/components/i18n/T';

export interface SidebarBadgeProps {
  count?: number | string;
  tone?: 'positive' | 'caution' | 'critical' | 'info' | 'accent' | 'neutral';
  locked?: boolean;
}

const TONE: Record<NonNullable<SidebarBadgeProps['tone']>, string> = {
  positive: 'glass-tint-positive text-positive border-positive/40',
  caution:  'glass-tint-caution text-caution border-caution/40',
  critical: 'glass-tint-critical text-critical border-critical/40',
  info:     'glass-tint-info text-info border-info/40',
  accent:   'glass-tint-accent text-accent border-accent/40',
  neutral:  'glass-panel text-mute border-rule',
};

export function SidebarBadge({ count, tone = 'neutral', locked }: SidebarBadgeProps) {
  if (locked) {
    return (
      <span className="glass-panel inline-flex items-center justify-center w-5 h-5 text-mute ml-auto">
        <Icon name="lock" size={10} />
      </span>
    );
  }
  if (count == null) return null;
  return (
    <span className={['inline-flex items-center justify-center min-w-5 h-5 px-1.5 ml-auto text-sm font-mono tabular-nums border', TONE[tone] || TONE.neutral].join(' ')}>
      {typeof count === 'number' && count > 99 ? '99+' : count}
    </span>
  );
}

export interface SidebarItemProps {
  icon: IconName;
  label: React.ReactNode;
  href: string;
  active?: boolean;
  locked?: boolean;
  badge?: React.ReactNode;
}

function NavRow({ icon, label, href, active, locked, badge }: SidebarItemProps) {
  const baseRow = 'group relative flex items-center gap-3 h-9 px-3 text-sm font-medium transition-colors border-l-2';
  const state = active
    ? 'glass-panel border-l-accent text-ink'
    : locked
    ? 'border-l-transparent text-mute opacity-60 cursor-not-allowed'
    : 'border-l-transparent text-ink-2 hover:bg-paper-2 hover:text-ink';
  const inner = (
    <>
      <span className="w-4 h-4 inline-flex items-center justify-center shrink-0">
        <Icon name={icon} size={16} className={active ? 'text-accent' : 'text-ink-2 group-hover:text-ink'} />
      </span>
      <span className="truncate flex-1">{label}</span>
      {badge}
    </>
  );
  if (locked) return <div className={[baseRow, state].join(' ')}>{inner}</div>;
  return (
    <Link href={href} className={[baseRow, state].join(' ')} aria-current={active ? 'page' : undefined}>
      {inner}
    </Link>
  );
}

function labelNode(label: SidebarLabel): React.ReactNode {
  if (typeof label === 'string') return label;
  return <T id={label.id} />;
}

export interface SidebarProps {
  currentUser?: { fullname?: string | null; role_name?: string | null } | null;
}

export const Sidebar: React.FC<SidebarProps> = ({ currentUser }) => {
  const pathname = usePathname() || '/';
  const search = useSearchParams()?.toString() ?? '';
  const searchStr = search ? `?${search}` : '';
  const fullName = currentUser?.fullname || '';
  const roleLabel = currentUser?.role_name || '';

  return (
    <aside className="glass-panel-heavy hidden md:flex md:w-60 lg:w-64 shrink-0 border-r border-rule rounded-br-2xl flex-col sticky top-[3.5rem] self-start h-[calc(100vh-3.5rem)]">
      <nav className="flex-1 overflow-y-auto py-3">
        {SIDEBAR_GROUPS.map((section) => (
          <div key={section.key} className="px-2 py-1">
            <div className="px-3 pb-1 pt-2 text-sm font-mono uppercase tracking-widest text-ink-2">
              {labelNode(section.label)}
            </div>
            <div className="space-y-0.5">
              {section.items.map((item) => {
                const active = matchSidebar(pathname, item, searchStr);
                return (
                  <NavRow key={item.key} href={item.href} icon={item.icon} label={labelNode(item.label)} active={active} />
                );
              })}
            </div>
          </div>
        ))}
      </nav>

      {currentUser && (
        <div className="border-t border-rule px-4 py-3">
          <div className="flex items-center gap-2.5">
            <div className="glass-panel w-8 h-8 inline-flex items-center justify-center rounded-sm text-ink-2 font-mono text-sm">
              {(fullName || '?').trim().split(/\s+/).slice(0, 2).map((p) => p[0]?.toUpperCase()).join('')}
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium text-ink truncate">{fullName}</div>
              <div className="text-sm font-mono uppercase tracking-wider text-ink-2 truncate">
                {roleLabel}
              </div>
            </div>
          </div>
        </div>
      )}
    </aside>
  );
};

export default Sidebar;
