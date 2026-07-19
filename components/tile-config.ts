// Static tile types + thin re-exports.
//
// The catalog itself lives in perm.tiles (db/seed.sql).
// This file exists only for:
//   - TileDef / TileWithMeta type aliases (consumed by tile-config consumers)
//   - tileHref() slug parser (used in client navigation)
//   - tileFromRow() DB row → TileDef mapper (single source of truth)
//   - GROUP_LABEL / GROUP_ORDER (consumed by TileHub)
//   - targetLabel() request-target → display-name (consumed by Tile, TileHub)
//
// All access decisions go through perm.tiles.view_perm_id
// via the cascade in lib/perm/* → resolved by tileAccess.ts.
// There is no TS-side role / department gating.
//
// Group values must match perm.tiles.group_name in the database.

import {
  BarChart3,
  BookOpen,
  Bot,
  BriefcaseBusiness,
  Building2,
  CalendarDays,
  ChartNoAxesCombined,
  CircleGauge,
  FileSearch,
  FileText,
  GitCompareArrows,
  Inbox,
  KeyRound,
  Landmark,
  LayoutDashboard,
  MessageCircleMore,
  Network,
  PackageCheck,
  ReceiptText,
  Scale,
  ScrollText,
  Search,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Target,
  Upload,
  UserRound,
  UsersRound,
  type LucideIcon,
} from 'lucide-react';
import type { IconName } from '@/components/icon';
import { iconByName } from '@/components/icon';

export type TileGroup =
  | 'work'
  | 'finance'
  | 'people'
  | 'legal'
  | 'exec'
  | 'admin'
  | 'self';

export interface TileAccessMeta {
  viewPermId?: string | null;
  dept_name?: string | null;
  group_name?: string | null;
}

export interface TileDef {
  id: string;
  display_name: string;
  subtitle: string;
  icon: string;
  accent: string;
  group: TileGroup;
  sub_view: string | null;
  href: string;
  module_id: string;
  request_target: string | null;
  sort_order: number;
  requestAccessTarget?: 'hr_manager' | 'cfo' | 'admin';
}

export interface TileWithMeta extends TileDef {
  count?: number | string;
  countLabel?: string;
  access_meta?: TileAccessMeta | null;
  group_name?: string | null;
  view_perm_id?: string | null;
}

export function tileHref(id: string): string {
  return '/' + id;
}

export const GROUP_LABEL: Record<TileGroup, { label: string; icon: IconName; id: string }> = {
  work:                   { label: 'Work',    icon: 'Briefcase', id: 'tiles.group.work.label' },
  finance:                { label: 'Finance', icon: 'BookOpen',  id: 'tiles.group.finance.label' },
  people:                 { label: 'People',  icon: 'Users',     id: 'tiles.group.people.label' },
  legal:                  { label: 'Legal',   icon: 'Scale',     id: 'tiles.group.legal.label' },
  exec:                   { label: 'Exec',    icon: 'Crown',     id: 'tiles.group.exec.label' },
  admin:                  { label: 'Admin',   icon: 'Settings',  id: 'tiles.group.admin.label' },
  self:                   { label: 'Self',    icon: 'User',      id: 'tiles.group.self.label' },
};

export function groupIcon(group: TileGroup): LucideIcon {
  return iconByName(GROUP_LABEL[group].icon);
}

const TILE_ICON: Record<string, LucideIcon> = {
  executive: ChartNoAxesCombined,
  inbox: Inbox,
  expense: ReceiptText,
  chat: MessageCircleMore,
  'sql-quick': Search,
  pr: FileText,
  po: PackageCheck,
  me_leave: CalendarDays,
  sales: BriefcaseBusiness,
  customers: UserRound,
  'search-coa': FileSearch,
  reconciliation: GitCompareArrows,
  ledger: Landmark,
  capital: Landmark,
  hr: UsersRound,
  hr_employees: UserRound,
  hr_leave: CalendarDays,
  law: Scale,
  law_admin: BookOpen,
  law_upload: Upload,
  cockpit: CircleGauge,
  summary: BarChart3,
  policy: Target,
  'org-chart': Network,
  roles: ShieldCheck,
  'tile-gates': SlidersHorizontal,
  directory: UsersRound,
  audit: ScrollText,
  departments: Building2,
  'access-requests': KeyRound,
  settings: Settings,
  hook: Bot,
};

const GROUP_ICON: Record<TileGroup, LucideIcon> = {
  work: Sparkles,
  finance: Landmark,
  people: UsersRound,
  legal: Scale,
  exec: LayoutDashboard,
  admin: Settings,
  self: UserRound,
};

export function tileIcon(tile: Pick<TileDef, 'id' | 'group'>): LucideIcon {
  return TILE_ICON[tile.id] ?? GROUP_ICON[tile.group] ?? LayoutDashboard;
}

export const GROUP_ORDER: TileGroup[] = [
  'work',
  'finance',
  'people',
  'legal',
  'exec',
  'admin',
  'self',
];

export function groupLabel(g: TileGroup): string {
  return GROUP_LABEL[g].label;
}

export function groupLabelId(g: TileGroup): string {
  return GROUP_LABEL[g].id;
}

export function groupLabelBi(g: TileGroup): { en: string; th: string; de: string } {
  const label = GROUP_LABEL[g].label;
  return { en: label, th: label, de: label };
}

export function targetLabel(t: { request_target?: string | null }): string {
  switch (t.request_target) {
    case 'cfo': return 'CFO';
    case 'admin': return 'Admin';
    case 'hr_manager':
    default: return 'HR Manager';
  }
}

export function requestAccessTargetToEnum(
  rt: string | null | undefined,
): 'hr_manager' | 'cfo' | 'admin' | undefined {
  if (rt === 'cfo') return 'cfo';
  if (rt === 'admin') return 'admin';
  if (rt === 'hr_manager') return 'hr_manager';
  return undefined;
}

export interface TileGrantSummary {
  tile_id: string;
  display_name: string;
  module_id: string;
  href: string;
  group_name: string;
  is_available: boolean;
  grant_reason: string;
  grant_source_role?: string | null;
  grant_role_level?: number | null;
}

export function tileAccentToTone(accent: string | null | undefined): {
  leftRule: string;
  bar: string;
  border: string;
  text: string;
} {
  const cls = (accent || '').toLowerCase();
  if (cls.includes('rose') || cls.includes('pink')) return { leftRule: 'border-l-critical', bar: 'bg-critical', border: 'border-critical/40', text: 'text-critical' };
  if (cls.includes('purple') || cls.includes('fuchsia')) return { leftRule: 'border-l-accent', bar: 'bg-accent-soft', border: 'border-accent/40', text: 'text-accent' };
  if (cls.includes('amber') || cls.includes('yellow')) return { leftRule: 'border-l-caution', bar: 'bg-caution', border: 'border-caution/40', text: 'text-caution' };
  if (cls.includes('cyan')  || cls.includes('sky'))    return { leftRule: 'border-l-info',    bar: 'bg-info',    border: 'border-info/40',    text: 'text-info' };
  if (cls.includes('indigo')|| cls.includes('violet')) return { leftRule: 'border-l-accent',  bar: 'bg-accent',  border: 'border-accent/40',  text: 'text-accent' };
  if (cls.includes('green') || cls.includes('emerald'))return { leftRule: 'border-l-positive',bar: 'bg-positive', border: 'border-positive/40',text: 'text-positive' };
  return { leftRule: 'border-l-rule', bar: 'bg-paper-2', border: 'border-rule/40', text: 'text-ink' };
}

export interface TileRow {
  id: string;
  display_name: string;
  subtitle: string;
  icon: string;
  accent: string;
  group_name: string;
  sub_view: string | null;
  href: string;
  view_perm_id: string;
  request_target: string | null;
  sort_order: number;
  module_id?: string;
  access_meta?: TileAccessMeta | null;
}

export function tileFromRow(t: TileRow): TileWithMeta {
  return {
    id: t.id,
    display_name: t.display_name,
    subtitle: t.subtitle,
    icon: t.icon,
    accent: t.accent,
    group: t.group_name as TileGroup,
    group_name: t.group_name,
    sub_view: t.sub_view,
    href: t.href,
    module_id: t.module_id ?? `tile:${t.id}`,
    view_perm_id: t.view_perm_id,
    request_target: t.request_target,
    sort_order: t.sort_order,
    requestAccessTarget: requestAccessTargetToEnum(t.request_target),
    access_meta: t.access_meta ?? { viewPermId: t.view_perm_id } as TileAccessMeta,
  };
}
