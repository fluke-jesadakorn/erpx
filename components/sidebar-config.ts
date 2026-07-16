import type { IconName } from '@/components/icons';

export type SidebarTone = 'positive' | 'caution' | 'critical' | 'info' | 'accent' | 'neutral';

export type SidebarLabel = string | { id: string };

export interface SidebarLink {
  key: string;
  label: SidebarLabel;
  icon: IconName;
  href: string;
  match?: (pathname: string) => boolean;
  badge?: number | string;
  badgeTone?: SidebarTone;
  perms?: string[];
  locked?: boolean;
}

export interface SidebarSection {
  key: string;
  label: SidebarLabel;
  items: SidebarLink[];
}

export const SIDEBAR_GROUPS: SidebarSection[] = [
  {
    key: 'home',
    label: { id: 'sidebar.home' },
    items: [
      { key: 'hub', label: { id: 'sidebar.labels.hub' }, icon: 'home', href: '/' },
    ],
  },
  {
    key: 'ai',
    label: { id: 'sidebar.aiChat' },
    items: [
      { key: 'chat', label: { id: 'sidebar.labels.chat' }, icon: 'zap', href: '/chat' },
    ],
  },
  {
    key: 'approvals',
    label: { id: 'sidebar.approvals' },
    items: [
      { key: 'inbox', label: { id: 'sidebar.labels.inbox' }, icon: 'inbox',     href: '/inbox?scope=waiting' },
      { key: 'queue', label: { id: 'sidebar.labels.queue' }, icon: 'clock',     href: '/inbox?scope=waiting' },
      { key: 'mine',  label: { id: 'sidebar.labels.mine' },  icon: 'file-text', href: '/inbox?scope=watching' },
      { key: 'all',   label: { id: 'sidebar.labels.all' },   icon: 'layers',    href: '/inbox?scope=all' },
    ],
  },
  {
    key: 'finance',
    label: { id: 'sidebar.finance' },
    items: [
      { key: 'cockpit', label: { id: 'sidebar.labels.cockpit' }, icon: 'gauge',   href: '/cockpit' },
      { key: 'expense', label: { id: 'sidebar.labels.expense' }, icon: 'receipt', href: '/expense' },
      { key: 'pr',      label: { id: 'sidebar.labels.pr' },      icon: 'package', href: '/pr' },
      { key: 'po',      label: { id: 'sidebar.labels.po' },      icon: 'truck',   href: '/po' },
    ],
  },
  {
    key: 'procurement',
    label: { id: 'sidebar.procurement' },
    items: [
      { key: 'sales', label: { id: 'sidebar.labels.sales' }, icon: 'shopping-cart', href: '/sales' },
    ],
  },
  {
    key: 'policy',
    label: { id: 'sidebar.policy' },
    items: [
      { key: 'policy', label: { id: 'sidebar.labels.policy' }, icon: 'shield-check', href: '/policy' },
      { key: 'roles',  label: { id: 'sidebar.labels.roles' },  icon: 'user-check',   href: '/roles' },
      { key: 'tiles',  label: { id: 'sidebar.labels.tiles' },  icon: 'grid',         href: '/tiles' },
    ],
  },
  {
    key: 'people',
    label: { id: 'sidebar.people' },
    items: [
      { key: 'audit',     label: { id: 'sidebar.labels.audit' },     icon: 'history',  href: '/audit' },
      { key: 'customers', label: { id: 'sidebar.labels.customers' }, icon: 'building', href: '/customers' },
    ],
  },
  {
    key: 'admin',
    label: { id: 'sidebar.admin' },
    items: [
      { key: 'system', label: { id: 'sidebar.labels.system' }, icon: 'settings', href: '/cockpit?view=admin' },
    ],
  },
];

export function matchSidebar(pathname: string, link: SidebarLink, search = ''): boolean {
  if (link.match) return link.match(pathname);
  if (link.href === '/') return pathname === '/' || pathname === '';
  const [basePath, ...rest] = link.href.split('?');
  const linkSearch = rest.length ? '?' + rest.join('?') : '';
  if (linkSearch) {
    return pathname === basePath && search === linkSearch;
  }
  return pathname === link.href || pathname.startsWith(link.href + '/') || pathname === basePath;
}
