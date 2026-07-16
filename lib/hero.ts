import type { TileWithMeta, TileGroup, TileAccessMeta } from '@/components/tile-config';

export type GreetingKey = 'morning' | 'afternoon' | 'evening';

export function timeGreeting(now: Date = new Date()): GreetingKey {
  const hour = Number(
    new Intl.DateTimeFormat('en-GB', {
      hour: '2-digit',
      hour12: false,
      timeZone: 'Asia/Bangkok',
    }).format(now),
  );
  if (hour >= 5 && hour < 12) return 'morning';
  if (hour >= 12 && hour < 18) return 'afternoon';
  return 'evening';
}

export function greetingLine(_name: string, key: GreetingKey = timeGreeting()): GreetingKey {
  return key;
}

export function pickPendingApprovals(prs: any[]): any[] {
  if (!Array.isArray(prs) || prs.length === 0) return [];
  const pending = prs.filter((pr) => {
    const s = String(pr?.status ?? '');
    if (!s) return false;
    if (s === 'approved' || s === 'rejected' || s === 'paid' || s === 'draft') return false;
    return s.endsWith('_review') || s === 'po_pending';
  });
  pending.sort((a, b) => {
    const ta = new Date(a?.created_at ?? 0).getTime();
    const tb = new Date(b?.created_at ?? 0).getTime();
    return ta - tb;
  });
  return pending;
}

export interface HeroKpis {
  open: number;
  locked: number;
  groups: number;
  total: number;
  groupLabels: string[];
}

export function kpiSummary(
  tiles: TileWithMeta[],
  isLocked: (t: TileWithMeta) => boolean,
): HeroKpis {
  const total = tiles.length;
  let locked = 0;
  const groupSet = new Set<TileGroup>();
  for (const t of tiles) {
    if (isLocked(t)) locked += 1;
    if (t.group) groupSet.add(t.group);
  }
  return {
    total,
    locked,
    open: Math.max(0, total - locked),
    groups: groupSet.size,
    groupLabels: Array.from(groupSet),
  };
}

export function metaLine(meta: TileAccessMeta | null | undefined): string {
  if (!meta) return '';
  const parts: string[] = [];
  if (meta.dept_name) parts.push(meta.dept_name);
  if (meta.group_name) parts.push(meta.group_name);
  return parts.join(' · ');
}
