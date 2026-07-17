'use client';

import React from 'react';
import Link from 'next/link';
import { usePathname, useSearchParams } from 'next/navigation';
import { Icon, type IconName } from '@/components/icons';
import { T } from '@/components/i18n/T';
import { matchSidebar, SIDEBAR_GROUPS, type SidebarLabel, type SidebarLink, type SidebarSection } from './sidebar-config';
import { MobileDrawer } from './MobileDrawer';

export interface SidebarBadgeProps {
  count?: number | string;
  tone?: 'positive' | 'caution' | 'critical' | 'info' | 'accent' | 'neutral';
  locked?: boolean;
}

const TONE: Record<NonNullable<SidebarBadgeProps['tone']>, string> = {
  positive: 'bg-positive-soft text-positive border-positive/40',
  caution: 'bg-caution-soft text-caution border-caution/40',
  critical: 'bg-critical-soft text-critical border-critical/40',
  info: 'bg-info-soft text-info border-info/40',
  accent: 'bg-accent-soft text-accent border-accent/40',
  neutral: 'bg-paper-3 text-mute border-rule',
};

export function SidebarBadge({ count, tone = 'neutral', locked }: SidebarBadgeProps) {
  if (locked) {
    return (
      <span className="ml-auto inline-flex h-5 w-5 items-center justify-center rounded-md border border-rule bg-paper-3 text-mute">
        <Icon name="lock" size={10} />
      </span>
    );
  }
  if (count == null) return null;
  return (
    <span className={[
      'ml-auto inline-flex h-5 min-w-5 items-center justify-center rounded-md border px-1.5 text-[11px] tabular-nums',
      TONE[tone] || TONE.neutral,
    ].join(' ')}>
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
  const base = 'group relative flex min-h-11 items-stretch gap-3 rounded-lg pl-3 pr-2.5 py-2 transition-colors';
  const state = active
    ? 'bg-accent-soft text-ink ring-1 ring-accent/20'
    : locked
      ? 'text-mute opacity-60 cursor-not-allowed'
      : 'text-ink-2 hover:bg-paper-3/60 hover:text-ink';
  const inner = (
    <>
      <span
        aria-hidden
        className={[
          'absolute left-0 top-2 bottom-2 w-[3px] rounded-full transition-colors',
          active ? 'bg-accent' : 'bg-transparent',
        ].join(' ')}
      />
      <span className={[
        'flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border transition-colors',
        active
          ? 'border-accent/40 bg-paper text-accent'
          : 'border-rule bg-paper text-ink-2 group-hover:border-rule-strong group-hover:text-ink',
      ].join(' ')}>
        <Icon name={icon} size={16} />
      </span>
      <span className="flex min-w-0 flex-1 flex-col justify-center text-left">
        {label}
      </span>
      {locked ? (
        <Icon name="lock" size={12} className="self-center text-mute" />
      ) : (
        badge
      )}
    </>
  );

  if (locked) return <div className={[base, state, 'cursor-not-allowed'].join(' ')}>{inner}</div>;
  return (
    <Link href={href} className={[base, state].join(' ')} aria-current={active ? 'page' : undefined}>
      {inner}
    </Link>
  );
}

function labelNode(label: SidebarLabel): React.ReactNode {
  return typeof label === 'string' ? label : <T id={label.id} variant="stacked" />;
}

function itemBadge(item: SidebarLink) {
  return <SidebarBadge count={item.badge} tone={item.badgeTone} />;
}

export interface SidebarProps {
  currentUser?: { fullname?: string | null; role_name?: string | null } | null;
}

function initials(name: string) {
  return (name || '?')
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase())
    .join('');
}

const COLLAPSE_STORAGE_KEY = 'folio.sidebar.collapsed';

function readCollapsed(): Record<string, boolean> {
  if (typeof window === 'undefined') return {};
  try {
    const raw = window.sessionStorage.getItem(COLLAPSE_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

function writeCollapsed(map: Record<string, boolean>) {
  if (typeof window === 'undefined') return;
  try {
    window.sessionStorage.setItem(COLLAPSE_STORAGE_KEY, JSON.stringify(map));
  } catch {
    /* ignore quota / privacy errors */
  }
}

function useCollapsedGroups() {
  const [map, setMap] = React.useState<Record<string, boolean>>({});
  const [hydrated, setHydrated] = React.useState(false);

  React.useEffect(() => {
    setMap(readCollapsed());
    setHydrated(true);
  }, []);

  const set = React.useCallback((key: string, value: boolean) => {
    setMap((prev) => {
      const next = { ...prev, [key]: value };
      writeCollapsed(next);
      return next;
    });
  }, []);

  const toggle = React.useCallback(
    (key: string) => set(key, !map[key]),
    [map, set],
  );

  const collapseAll = React.useCallback(() => {
    const next: Record<string, boolean> = {};
    for (const s of SIDEBAR_GROUPS) next[s.key] = true;
    setMap(() => {
      writeCollapsed(next);
      return next;
    });
  }, []);

  const expandAll = React.useCallback(() => {
    setMap(() => {
      writeCollapsed({});
      return {};
    });
  }, []);

  return { map, set, toggle, collapseAll, expandAll, hydrated };
}

function SectionGroup({
  section,
  pathname,
  search,
  collapsed,
  onToggle,
}: {
  section: SidebarSection;
  pathname: string;
  search: string;
  collapsed: boolean;
  onToggle: () => void;
}) {
  const hasActive = section.items.some((item) => matchSidebar(pathname, item, search));
  const forceOpen = collapsed && hasActive;
  const showItems = !collapsed || forceOpen;

  return (
    <section>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={!collapsed}
        className={[
          'group flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors',
          'hover:bg-paper-3/60 focus-visible:bg-paper-3/60',
        ].join(' ')}
      >
        <span aria-hidden className="h-1 w-1 rounded-full bg-accent/70" />
        <span className="text-[11px] font-medium uppercase tracking-wider text-mute transition-colors group-hover:text-ink-2">
          {labelNode(section.label)}
        </span>
        <span className="ml-auto flex items-center gap-1.5 text-mute">
          {forceOpen && (
            <span aria-hidden className="h-1.5 w-1.5 rounded-full bg-accent" title="Active section" />
          )}
          <Icon
            name="chevron-down"
            size={12}
            className={[
              'transition-transform duration-200',
              collapsed ? '-rotate-90' : 'rotate-0',
            ].join(' ')}
          />
        </span>
      </button>
      <div
        className={[
          'grid transition-[grid-template-rows] duration-200 ease-out',
          showItems ? 'grid-rows-[1fr] mt-0.5' : 'grid-rows-[0fr]',
        ].join(' ')}
      >
        <div className="overflow-hidden">
          <div className="space-y-0.5">
            {section.items.map((item) => (
              <NavRow
                key={item.key}
                href={item.href}
                icon={item.icon}
                label={labelNode(item.label)}
                active={matchSidebar(pathname, item, search)}
                locked={item.locked}
                badge={itemBadge(item)}
              />
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}

function SidebarBody({ currentUser }: SidebarProps) {
  const pathname = usePathname() || '/';
  const params = useSearchParams();
  const search = params ? `?${params.toString()}` : '';
  const name = currentUser?.fullname || '';
  const role = currentUser?.role_name || '';
  const { map, toggle, collapseAll, expandAll, hydrated } = useCollapsedGroups();
  const allCollapsed = hydrated && SIDEBAR_GROUPS.every((s) => map[s.key]);

  return (
    <>
      <nav
        className="min-h-0 flex-1 overflow-y-auto px-3 pb-2"
        aria-label="Primary navigation"
      >
        <div className="space-y-1.5">
          {SIDEBAR_GROUPS.map((section) => (
            <SectionGroup
              key={section.key}
              section={section}
              pathname={pathname}
              search={search}
              collapsed={hydrated ? Boolean(map[section.key]) : false}
              onToggle={() => toggle(section.key)}
            />
          ))}
        </div>
      </nav>

      <div className="px-3 pb-2">
        <button
          type="button"
          onClick={allCollapsed ? expandAll : collapseAll}
          aria-label={allCollapsed ? 'Expand all sections' : 'Collapse all sections'}
          className="group flex w-full items-center justify-center gap-2 rounded-md border border-dashed border-rule py-1.5 text-[11px] font-medium uppercase tracking-wider text-mute transition-colors hover:border-rule-strong hover:bg-paper-3/60 hover:text-ink-2"
        >
          <Icon
            name={allCollapsed ? 'chevron-down' : 'chevron-up'}
            size={12}
            className="transition-transform group-hover:scale-110"
          />
          <span>{allCollapsed ? 'Expand all' : 'Collapse all'}</span>
        </button>
      </div>

      {currentUser && (
        <div className="m-2 mt-1 rounded-xl border border-rule bg-paper p-3 shadow-[var(--shadow-panel)]">
          <div className="flex items-center gap-3">
            <div className="relative shrink-0">
              <div className="flex h-10 w-10 items-center justify-center rounded-full border border-rule bg-gradient-to-br from-accent-soft to-paper-3 text-sm font-semibold text-accent">
                {initials(name)}
              </div>
              <span aria-hidden className="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-paper bg-positive" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="truncate text-sm font-medium text-ink">{name}</div>
              <div className="truncate text-xs text-mute">{role}</div>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

export const Sidebar: React.FC<SidebarProps> = ({ currentUser }) => {
  const [open, setOpen] = React.useState(false);
  const pathname = usePathname() || '/';

  React.useEffect(() => {
    const onOpen = () => setOpen(true);
    window.addEventListener('folio:open-sidebar', onOpen);
    return () => window.removeEventListener('folio:open-sidebar', onOpen);
  }, []);

  React.useEffect(() => {
    setOpen(false);
  }, [pathname]);

  return (
    <>
      <aside
        className="sticky top-14 hidden h-[calc(100vh-3.5rem)] w-64 shrink-0 flex-col border-r border-rule bg-paper-2/80 backdrop-blur supports-[backdrop-filter]:bg-paper-2/70 lg:flex"
        style={{
          backgroundImage:
            'linear-gradient(180deg, color-mix(in oklab, var(--paper-2) 92%, var(--paper)) 0%, var(--paper-2) 100%)',
        }}
      >
        <SidebarBody currentUser={currentUser} />
      </aside>
      <div className="lg:hidden">
        <MobileDrawer open={open} onClose={() => setOpen(false)}>
          <SidebarBody currentUser={currentUser} />
        </MobileDrawer>
      </div>
    </>
  );
};

export default Sidebar;
