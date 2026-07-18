'use client';

import React from 'react';
import Link from 'next/link';
import { usePathname, useSearchParams } from 'next/navigation';
import { useTranslations } from 'next-intl';
import { ChevronDown, ChevronUp, Lock, type LucideIcon } from 'lucide-react';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { matchSidebar, SIDEBAR_GROUPS, type SidebarLabel, type SidebarLink, type SidebarSection } from './sidebar-config';
import { MobileDrawer } from './MobileDrawer';
import thDict from '../messages/th.json';
import deDict from '../messages/de.json';
import { matchPerm } from '@/perm';

type Dict = Record<string, unknown>;

function lookup(dict: Dict, path: string): string | undefined {
  const parts = path.split('.');
  let cur: unknown = dict;
  for (const p of parts) {
    if (cur && typeof cur === 'object' && p in (cur as Record<string, unknown>)) {
      cur = (cur as Record<string, unknown>)[p];
    } else {
      return undefined;
    }
  }
  return typeof cur === 'string' ? cur : undefined;
}

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

type GroupTone = {
  text: string;
  soft: string;
  active: string;
  border: string;
  dot: string;
};

const GROUP_TONE: Record<string, GroupTone> = {
  home: { text: 'text-info', soft: 'bg-info-soft/20', active: 'bg-info-soft/65', border: 'border-info/60', dot: 'bg-info' },
  ai: { text: 'text-accent', soft: 'bg-accent-soft/20', active: 'bg-accent-soft/65', border: 'border-accent/60', dot: 'bg-accent' },
  approvals: { text: 'text-caution', soft: 'bg-caution-soft/20', active: 'bg-caution-soft/65', border: 'border-caution/60', dot: 'bg-caution' },
  finance: { text: 'text-positive', soft: 'bg-positive-soft/20', active: 'bg-positive-soft/65', border: 'border-positive/60', dot: 'bg-positive' },
  procurement: { text: 'text-info', soft: 'bg-info-soft/20', active: 'bg-info-soft/65', border: 'border-info/60', dot: 'bg-info' },
  policy: { text: 'text-accent', soft: 'bg-accent-soft/20', active: 'bg-accent-soft/65', border: 'border-accent/60', dot: 'bg-accent' },
  people: { text: 'text-positive', soft: 'bg-positive-soft/20', active: 'bg-positive-soft/65', border: 'border-positive/60', dot: 'bg-positive' },
  executive: { text: 'text-caution', soft: 'bg-caution-soft/20', active: 'bg-caution-soft/65', border: 'border-caution/60', dot: 'bg-caution' },
};

export function SidebarBadge({ count, tone = 'neutral', locked }: SidebarBadgeProps) {
  if (locked) {
    return (
      <span className="ml-auto inline-flex h-4 w-4 items-center justify-center rounded-md border border-rule bg-paper-3 text-mute">
        <Lock size={9} />
      </span>
    );
  }
  if (count == null) return null;
  return (
    <span className={[
      'ml-auto inline-flex h-4 min-w-4 items-center justify-center rounded-md border px-1 text-[10px] tabular-nums',
      TONE[tone] || TONE.neutral,
    ].join(' ')}>
      {typeof count === 'number' && count > 99 ? '99+' : count}
    </span>
  );
}

export interface SidebarItemProps {
  icon: LucideIcon;
  label: React.ReactNode;
  href: string;
  active?: boolean;
  locked?: boolean;
  badge?: React.ReactNode;
  tone?: GroupTone;
}

function NavRow({ icon: IconCmp, label, href, active, locked, badge, tone = GROUP_TONE.home }: SidebarItemProps) {
  const base = 'group relative flex min-h-10 items-stretch gap-3 rounded-lg border border-transparent pl-3.5 pr-3 py-1.5 transition-all duration-200';
  const state = active
    ? `${tone.border} ${tone.active} text-ink shadow-[var(--shadow-panel)]`
    : locked
      ? 'text-mute/70 opacity-60 cursor-not-allowed'
      : 'text-ink-2 hover:border-rule/60 hover:bg-paper-3/60 hover:text-ink';
  const inner = (
    <>
      <span
        aria-hidden
        className={[
          'absolute left-0.5 top-2 bottom-2 w-0.5 rounded-full transition-colors',
          active ? tone.dot : 'bg-transparent',
        ].join(' ')}
      />
      <span
        aria-hidden
        className={[
          'flex h-5 w-5 shrink-0 items-center justify-center transition-colors',
          tone.text,
        ].join(' ')}
      >
        <IconCmp size={14} />
      </span>
      <span className="flex min-w-0 flex-1 items-center text-left">
        {label}
      </span>
      {locked ? (
        <Lock size={11} className="self-center text-mute/70" />
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

function BilingualLabel({ label, active, section = false, activeTone }: { label: SidebarLabel; active?: boolean; section?: boolean; activeTone?: string }) {
  const t = useTranslations();
  const loc = useSecondaryLocale();
  const primaryClass = section
    ? ['truncate text-[14px] font-semibold leading-tight tracking-[0.02em]', active ? activeTone || 'text-accent-strong' : 'text-mute/85'].join(' ')
    : ['truncate text-[13px] font-medium', active ? activeTone || 'text-ink' : 'text-ink-2'].join(' ');

  if (typeof label === 'string') {
    return <span className={primaryClass}>{label}</span>;
  }

  const primary = t(label.id) || '';
  const dict = loc === 'th' ? (thDict as Dict) : (deDict as Dict);
  const secondary = lookup(dict, label.id);
  const showSecondary = secondary && secondary !== primary;

  if (!showSecondary) {
    return <span className={primaryClass}>{primary}</span>;
  }

  return (
    <span className="flex min-w-0 flex-col justify-center leading-tight">
      <span className={primaryClass}>
        {primary}
      </span>
      <span className={section ? 'truncate text-[10px] font-normal text-mute/90' : 'truncate text-[10.5px] font-normal text-mute/90'} lang={loc}>
        {secondary}
      </span>
    </span>
  );
}

function itemBadge(item: SidebarLink) {
  return <SidebarBadge count={item.badge} tone={item.badgeTone} />;
}

export interface SidebarProps {
  currentUser?: { fullname?: string | null; role_name?: string | null; permissions?: string[] } | null;
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
  const sectionActive = section.items.some((item) => matchSidebar(pathname, item, search));
  const tone = GROUP_TONE[section.key] || GROUP_TONE.home;
  const SectionIcon = section.icon;

  if (section.items.length === 1) {
    const item = section.items[0];
    return (
      <NavRow
        href={item.href}
        icon={item.icon}
        label={<BilingualLabel label={item.label} active={matchSidebar(pathname, item, search)} activeTone={tone.text} />}
        active={matchSidebar(pathname, item, search)}
        locked={item.locked}
        badge={itemBadge(item)}
        tone={tone}
      />
    );
  }

  const forceOpen = collapsed && sectionActive;
  const showItems = !collapsed || forceOpen;

  return (
    <section>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={!collapsed}
        className={[
          'group flex w-full items-center gap-2.5 rounded-lg border border-transparent px-2.5 py-2 text-left transition-all duration-200',
          'hover:border-rule/60 hover:bg-paper-3/50 focus-visible:bg-paper-3/60',
          sectionActive ? `${tone.active} ${tone.border}` : tone.soft,
        ].join(' ')}
      >
        <span
          aria-hidden
          className={[
            'flex h-6 w-6 shrink-0 items-center justify-center transition-colors',
            tone.text,
          ].join(' ')}
        >
          <SectionIcon size={15} />
        </span>
        <span className="min-w-0 flex-1 text-[13px] font-semibold uppercase tracking-[0.1em] transition-opacity group-hover:opacity-100">
          <BilingualLabel label={section.label} active={sectionActive} section activeTone={tone.text} />
        </span>
        <span className={['ml-auto flex items-center gap-2', tone.text].join(' ')}>
          {forceOpen && (
            <span aria-hidden className={['h-1.5 w-1.5 rounded-full', tone.dot].join(' ')} title="Active section" />
          )}
          <ChevronDown
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
          showItems ? 'grid-rows-[1fr] mt-1' : 'grid-rows-[0fr]',
        ].join(' ')}
      >
        <div className="overflow-hidden">
          <div className="space-y-0.5">
            {section.items.map((item) => (
              <NavRow
                key={item.key}
                href={item.href}
                icon={item.icon}
                label={<BilingualLabel label={item.label} active={matchSidebar(pathname, item, search)} activeTone={tone.text} />}
                active={matchSidebar(pathname, item, search)}
                locked={item.locked}
                badge={itemBadge(item)}
                tone={tone}
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
  const groups = React.useMemo(
    () => {
      const perms = currentUser?.permissions ?? [];
      return SIDEBAR_GROUPS
        .map((section) => ({
          ...section,
          items: section.items.filter((item) => !item.perms?.length || item.perms.some((perm) => matchPerm(perms, perm))),
        }))
        .filter((section) => section.items.length > 0);
    },
    [currentUser?.permissions],
  );
  const { map, toggle, collapseAll, expandAll, hydrated } = useCollapsedGroups();
  const allCollapsed = hydrated && SIDEBAR_GROUPS.every((s) => map[s.key]);

  return (
    <>
      <nav
        className="min-h-0 flex-1 overflow-y-auto px-3 pb-3"
        aria-label="Primary navigation"
      >
        <div className="space-y-1.5 divide-y divide-rule/60">
          {groups.map((section, idx) => (
            <div key={section.key} className={idx === 0 ? '' : 'pt-2'}>
              <SectionGroup
                section={section}
                pathname={pathname}
                search={search}
                collapsed={hydrated ? Boolean(map[section.key]) : false}
                onToggle={() => toggle(section.key)}
              />
            </div>
          ))}
        </div>
      </nav>

      <div className="px-3 pb-2.5">
        <button
          type="button"
          onClick={allCollapsed ? expandAll : collapseAll}
          aria-label={allCollapsed ? 'Expand all sections' : 'Collapse all sections'}
          className="group flex w-full items-center justify-center gap-2 rounded-lg border border-accent/25 bg-accent-soft/15 py-1.5 text-[10px] font-medium uppercase tracking-[0.1em] text-accent-strong transition-colors hover:border-accent/50 hover:bg-accent-soft/35 hover:text-accent-strong"
        >
          {allCollapsed ? (
            <ChevronDown size={11} className="transition-transform group-hover:scale-110" />
          ) : (
            <ChevronUp size={11} className="transition-transform group-hover:scale-110" />
          )}
          <span>{allCollapsed ? 'Expand all' : 'Collapse all'}</span>
        </button>
      </div>

      {currentUser && (
        <div className="mx-3 mb-3 mt-1 rounded-md border border-positive/30 bg-positive-soft/20 p-3 shadow-[var(--shadow-panel)]">
          <div className="flex items-center gap-2.5">
            <div className="relative shrink-0">
              <div className="flex h-9 w-9 items-center justify-center rounded-full border border-rule  from-accent-soft to-paper-3 text-xs font-semibold text-accent">
                {initials(name)}
              </div>
              <span aria-hidden className="absolute -bottom-0.5 -right-0.5 h-2 w-2 rounded-full border-2 border-paper bg-positive" />
            </div>
            <div className="min-w-0 flex-1" title={role}>
              <div className="truncate text-[13px] font-medium text-ink">{name}</div>
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
        className="panel-floating sticky top-16 ml-3 hidden h-[calc(100vh-5rem)] w-64 shrink-0 flex-col overflow-hidden rounded-2xl lg:flex"
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
