// Static tile types + thin re-exports.
//
// The catalog itself lives in perm.tiles (db/perm/*).
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

export const GROUP_LABEL: Record<TileGroup, { label: string; icon: string; id: string }> = {
  work:                   { label: 'Work',    icon: '🧰', id: 'tiles.group.work.label' },
  finance:                { label: 'Finance', icon: '📒', id: 'tiles.group.finance.label' },
  people:                 { label: 'People',  icon: '👥', id: 'tiles.group.people.label' },
  legal:                  { label: 'Legal',   icon: '⚖️', id: 'tiles.group.legal.label' },
  exec:                   { label: 'Exec',    icon: '👑', id: 'tiles.group.exec.label' },
  admin:                  { label: 'Admin',   icon: '⚙️', id: 'tiles.group.admin.label' },
  self:                   { label: 'Self',    icon: '👤', id: 'tiles.group.self.label' },
};

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
  if (cls.includes('rose') || cls.includes('pink')) return { leftRule: 'border-l-rose-500',     bar: 'bg-rose-500',     border: 'border-rose-500/40',  text: 'text-rose-200' };
  if (cls.includes('purple') || cls.includes('fuchsia')) return { leftRule: 'border-l-purple-500', bar: 'bg-purple-500',   border: 'border-purple-500/40', text: 'text-purple-200' };
  if (cls.includes('amber') || cls.includes('yellow')) return { leftRule: 'border-l-amber-500',  bar: 'bg-amber-500',    border: 'border-amber-500/40', text: 'text-amber-200' };
  if (cls.includes('cyan')  || cls.includes('sky'))    return { leftRule: 'border-l-cyan-500',   bar: 'bg-cyan-500',     border: 'border-cyan-500/40',  text: 'text-cyan-200' };
  if (cls.includes('indigo')|| cls.includes('violet')) return { leftRule: 'border-l-indigo-500', bar: 'bg-indigo-500',   border: 'border-indigo-500/40', text: 'text-indigo-200' };
  if (cls.includes('green') || cls.includes('emerald'))return { leftRule: 'border-l-emerald-500',bar: 'bg-emerald-500',  border: 'border-emerald-500/40',text: 'text-emerald-200' };
  return { leftRule: 'border-l-slate-500', bar: 'bg-slate-500', border: 'border-slate-500/40', text: 'text-slate-200' };
}

export function tileIconFromEmoji(emoji: string | null | undefined): string {
  if (!emoji) return 'square';
  return 'square';
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