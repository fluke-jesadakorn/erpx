import type { ReactNode } from 'react';
import type { IconName } from '@/components/icon';
import { GROUP_LABEL, type TileGroup, type TileWithMeta } from './tile-config';

export interface Crumb {
  label: ReactNode;
  href?: string;
  icon?: IconName;
}

export const ROOT_CRUMB: Crumb = { label: 'Folio', href: '/', icon: 'Globe' };

export const groupCrumb = (group: TileGroup): Crumb => ({
  label: GROUP_LABEL[group].label.toUpperCase(),
  href: `/group/${group}`,
  icon: GROUP_LABEL[group].icon,
});

export interface CrumbSpec {
  group: TileGroup;
  tile?: Pick<TileWithMeta, 'id' | 'display_name' | 'sub_view'>;
  subView?: string | null;
  record?: { label: string; href?: string };
}

export function buildCrumbs(spec: CrumbSpec): Crumb[] {
  const crumbs: Crumb[] = [ROOT_CRUMB, groupCrumb(spec.group)];
  if (spec.tile) {
    crumbs.push({ label: spec.tile.display_name, href: `/${spec.tile.id}` });
  }
  const subView = spec.subView ?? spec.tile?.sub_view;
  if (subView) {
    crumbs.push({ label: subView });
  }
  if (spec.record) {
    crumbs.push({ label: spec.record.label, href: spec.record.href });
  }
  return crumbs;
}

export function tileCrumbs(
  tile: { id: string; display_name: string; sub_view: string | null; group_name?: string | null; group?: TileGroup },
): Crumb[] {
  const group = (tile.group ?? (tile.group_name as TileGroup | undefined)) as TileGroup;
  return buildCrumbs({
    group,
    tile: { id: tile.id, display_name: tile.display_name, sub_view: tile.sub_view },
  });
}
