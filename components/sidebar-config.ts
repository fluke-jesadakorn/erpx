import {
  ArrowDownUp,
  BookOpen,
  Building2,
  Gauge,
  Home,
  Inbox,
  Landmark,
  LayoutGrid,
  Palette,
  Package,
  Receipt,
  ShieldCheck,
  ShoppingCart,
  Star,
  Scale,
  Settings,
  Truck,
  Users,
  Zap,
  History,
  type LucideIcon,
} from 'lucide-react';

export type SidebarTone = 'positive' | 'caution' | 'critical' | 'info' | 'accent' | 'neutral';

export type SidebarLabel = string | { id: string };

export interface SidebarLink {
  key: string;
  label: SidebarLabel;
  icon: LucideIcon;
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
  icon: LucideIcon;
  items: SidebarLink[];
}

export const SIDEBAR_GROUPS: SidebarSection[] = [
  {
    key: 'home',
    label: { id: 'sidebar.home' },
    icon: Home,
    items: [
      { key: 'hub', label: { id: 'sidebar.labels.hub' }, icon: Home, href: '/' },
    ],
  },
  {
    key: 'ai',
    label: { id: 'sidebar.aiChat' },
    icon: Zap,
    items: [
      { key: 'chat', label: { id: 'sidebar.labels.chat' }, icon: Zap, href: '/chat', perms: ['tile:chat:view::allow'] },
    ],
  },
  {
    key: 'approvals',
    label: { id: 'sidebar.approvals' },
    icon: Inbox,
    items: [
      { key: 'inbox', label: { id: 'sidebar.labels.inbox' }, icon: Inbox, href: '/inbox?scope=waiting', perms: ['tile:inbox:view::allow'] },
    ],
  },
  {
    key: 'finance',
    label: { id: 'sidebar.finance' },
    icon: Gauge,
    items: [
      { key: 'expense', label: { id: 'sidebar.labels.expense' }, icon: Receipt, href: '/expense', perms: ['tile:expense:view::allow'] },
      { key: 'pr',      label: { id: 'sidebar.labels.pr' },      icon: Package, href: '/pr', perms: ['tile:pr:view::allow'] },
      { key: 'po',      label: { id: 'sidebar.labels.po' },      icon: Truck,   href: '/po', perms: ['tile:po:view::allow'] },
      { key: 'capital', label: { id: 'sidebar.labels.capital' }, icon: Landmark, href: '/capital', perms: ['tile:capital:view::allow'] },
      { key: 'accounting', label: { id: 'sidebar.labels.accounting' }, icon: Gauge, href: '/accounting', perms: ['finance:journal:prepare::allow', 'finance:journal:approve::allow'] },
      { key: 'ledger', label: { id: 'sidebar.labels.ledger' }, icon: BookOpen, href: '/ledger', perms: ['finance:ledger:view::allow', 'finance:gl:view::allow'] },
      { key: 'inventory', label: { id: 'sidebar.labels.inventory' }, icon: Package, href: '/inventory', perms: ['inventory:stock:view::allow'] },
      { key: 'reconciliation', label: { id: 'sidebar.labels.reconciliation' }, icon: ArrowDownUp, href: '/reconciliation', perms: ['finance:bank:import::allow', 'finance:bank:match::allow'] },
      { key: 'reports', label: { id: 'sidebar.labels.reports' }, icon: Gauge, href: '/reports', perms: ['finance:report:view::allow', 'finance:cashflow:read::allow'] },
      { key: 'budgets', label: { id: 'sidebar.labels.budgets' }, icon: LayoutGrid, href: '/budgets', perms: ['finance:budget:view::allow', 'finance:budget:manage::allow'] },
    ],
  },
  {
    key: 'procurement',
    label: { id: 'sidebar.procurement' },
    icon: ShoppingCart,
    items: [
      { key: 'sales', label: { id: 'sidebar.labels.sales' }, icon: ShoppingCart, href: '/sales', perms: ['tile:sales:view::allow'] },
    ],
  },
  {
    key: 'policy',
    label: { id: 'sidebar.policy' },
    icon: ShieldCheck,
    items: [
      { key: 'policy', label: { id: 'sidebar.labels.policy' }, icon: ShieldCheck, href: '/policy', perms: ['rbac:matrix:view::allow'] },
      { key: 'tiles',  label: { id: 'sidebar.labels.tiles' },  icon: LayoutGrid,  href: '/tiles', perms: ['rbac:matrix:view::allow'] },
      { key: 'design-system', label: { id: 'sidebar.labels.designSystem' }, icon: Palette, href: '/design-system', perms: ['admin:system:bypass::allow'] },
      { key: 'settings', label: { id: 'sidebar.labels.settings' }, icon: Settings, href: '/ai-settings', perms: ['tile:settings:view::allow'] },
    ],
  },
  {
    key: 'people',
    label: { id: 'sidebar.people' },
    icon: Users,
    items: [
      { key: 'hr', label: { id: 'sidebar.labels.hr' }, icon: Users, href: '/hr', perms: ['tile:hr:view::allow'] },
      { key: 'law', label: { id: 'sidebar.labels.law' }, icon: Scale, href: '/law', perms: ['tile:law:view::allow'] },
      { key: 'audit',     label: { id: 'sidebar.labels.audit' },     icon: History,    href: '/audit', perms: ['tile:audit:view::allow'] },
      { key: 'customers', label: { id: 'sidebar.labels.customers' }, icon: Building2, href: '/customers', perms: ['tile:customers:view::allow'] },
    ],
  },
  {
    key: 'executive',
    label: { id: 'sidebar.executive' },
    icon: Star,
    items: [
      { key: 'overview', label: { id: 'sidebar.labels.executive' }, icon: Star, href: '/executive', perms: ['tile:executive:view::allow', 'finance:report:executive::allow'] },
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
