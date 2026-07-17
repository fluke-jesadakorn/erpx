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
  icon: IconName;
  items: SidebarLink[];
}

export const SIDEBAR_GROUPS: SidebarSection[] = [
  {
    key: 'home',
    label: { id: 'sidebar.home' },
    icon: 'home',
    items: [
      { key: 'hub', label: { id: 'sidebar.labels.hub' }, icon: 'home', href: '/' },
    ],
  },
  {
    key: 'ai',
    label: { id: 'sidebar.aiChat' },
    icon: 'zap',
    items: [
      { key: 'chat', label: { id: 'sidebar.labels.chat' }, icon: 'zap', href: '/chat' },
    ],
  },
  {
    key: 'approvals',
    label: { id: 'sidebar.approvals' },
    icon: 'inbox',
    items: [
      { key: 'inbox', label: { id: 'sidebar.labels.inbox' }, icon: 'inbox', href: '/inbox?scope=waiting' },
    ],
  },
  {
    key: 'finance',
    label: { id: 'sidebar.finance' },
    icon: 'gauge',
    items: [
      { key: 'expense', label: { id: 'sidebar.labels.expense' }, icon: 'receipt', href: '/expense' },
      { key: 'pr',      label: { id: 'sidebar.labels.pr' },      icon: 'package', href: '/pr' },
      { key: 'po',      label: { id: 'sidebar.labels.po' },      icon: 'truck',   href: '/po' },
    ],
  },
  {
    key: 'procurement',
    label: { id: 'sidebar.procurement' },
    icon: 'shopping-cart',
    items: [
      { key: 'sales', label: { id: 'sidebar.labels.sales' }, icon: 'shopping-cart', href: '/sales' },
    ],
  },
  {
    key: 'policy',
    label: { id: 'sidebar.policy' },
    icon: 'shield-check',
    items: [
      { key: 'policy', label: { id: 'sidebar.labels.policy' }, icon: 'shield-check', href: '/policy' },
      { key: 'roles',  label: { id: 'sidebar.labels.roles' },  icon: 'user-check',   href: '/roles' },
      { key: 'tiles',  label: { id: 'sidebar.labels.tiles' },  icon: 'grid',         href: '/tiles' },
    ],
  },
  {
    key: 'people',
    label: { id: 'sidebar.people' },
    icon: 'users',
    items: [
      { key: 'audit',     label: { id: 'sidebar.labels.audit' },     icon: 'history',  href: '/audit' },
      { key: 'customers', label: { id: 'sidebar.labels.customers' }, icon: 'building', href: '/customers' },
    ],
  },
  {
    key: 'executive',
    label: { id: 'sidebar.executive' },
    icon: 'star',
    items: [
      { key: 'overview', label: { id: 'sidebar.labels.executive' }, icon: 'star', href: '/executive' },
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