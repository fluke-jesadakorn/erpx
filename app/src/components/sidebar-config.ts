import type { IconName } from '@/components/icons';

export type SidebarTone = 'positive' | 'caution' | 'critical' | 'info' | 'accent' | 'neutral';

export interface SidebarLink {
  key: string;
  label: string;
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
  label: string;
  items: SidebarLink[];
}

export const SIDEBAR_GROUPS: SidebarSection[] = [
  {
    key: 'home',
    label: 'Home',
    items: [
      { key: 'hub', label: 'Hub', icon: 'home', href: '/' },
    ],
  },
  {
    key: 'approvals',
    label: 'Approvals',
    items: [
      { key: 'inbox', label: 'Inbox',    icon: 'inbox',     href: '/inbox' },
      { key: 'queue', label: 'My queue', icon: 'clock',     href: '/inbox?scope=waiting' },
      { key: 'mine',  label: 'Mine',     icon: 'file-text', href: '/inbox?scope=watching' },
      { key: 'all',   label: 'All',      icon: 'layers',    href: '/inbox?scope=all' },
    ],
  },
  {
    key: 'finance',
    label: 'Finance',
    items: [
      { key: 'cockpit', label: 'Overview', icon: 'gauge',   href: '/cockpit' },
      { key: 'expense', label: 'Expense',  icon: 'receipt', href: '/expense' },
      { key: 'pr',      label: 'PR',       icon: 'package', href: '/pr' },
      { key: 'po',      label: 'PO',       icon: 'truck',   href: '/po' },
    ],
  },
  {
    key: 'procurement',
    label: 'Procurement',
    items: [
      { key: 'sales', label: 'Sales orders', icon: 'shopping-cart', href: '/sales' },
    ],
  },
  {
    key: 'policy',
    label: 'Policy',
    items: [
      { key: 'policy', label: 'Policy matrix', icon: 'shield-check', href: '/policy' },
      { key: 'roles',  label: 'Roles',          icon: 'user-check',   href: '/roles' },
      { key: 'tiles',  label: 'Tile catalog',   icon: 'grid',         href: '/tiles' },
    ],
  },
  {
    key: 'people',
    label: 'People',
    items: [
      { key: 'audit',     label: 'Audit log', icon: 'history',  href: '/audit' },
      { key: 'customers', label: 'Customers', icon: 'building', href: '/customers' },
    ],
  },
  {
    key: 'admin',
    label: 'Admin',
    items: [
      { key: 'system', label: 'System', icon: 'settings', href: '/cockpit?view=admin' },
    ],
  },
];

export function matchSidebar(pathname: string, link: SidebarLink): boolean {
  if (link.match) return link.match(pathname);
  if (link.href === '/') return pathname === '/' || pathname === '';
  return pathname === link.href || pathname.startsWith(link.href + '/') || pathname === link.href.split('?')[0];
}
