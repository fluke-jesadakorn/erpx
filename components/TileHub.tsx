'use client';

import React, { useEffect, useMemo, useState } from 'react';
import { useTranslations } from 'next-intl';
import { TileTooltipProvider } from './TileTooltip';
import { SortableTileGrid } from './tiles/SortableTileGrid';
import {
  type TileDef,
  type TileWithMeta,
  type TileGroup,
  GROUP_LABEL,
  GROUP_ORDER,
  groupIcon,
  targetLabel,
} from './tile-config';
import {
  evaluateTileOptimistic,
  type TileAccess,
} from './tileAccess';
import { applyOrder, clearGroupOrder, hasGroupOrder, loadOrder, type TileOrderMap } from '@/tileOrder';
import { matchPerm } from '@/perm';
import { T } from '@/components/i18n/T';
import { CheckCircle2, LayoutGrid, LoaderCircle, Lock, Search, type LucideIcon } from 'lucide-react';

interface TileHubProps {
  currentUser: any;
  tiles?: TileDef[];
  activeTileId: string;
  onSelectTile: (t: any) => void;
  accessByTile?: Record<string, TileAccess>;
}

type Annotated = { tile: TileWithMeta; access: TileAccess };
type CatalogFilter = 'available' | 'all' | 'locked';

export const TileHub: React.FC<TileHubProps> = ({
  currentUser,
  tiles: tilesProp,
  activeTileId,
  onSelectTile,
  accessByTile,
}) => {
  const t = useTranslations();
  const userPerms = useMemo(() => currentUser?.permissions ?? [], [currentUser?.permissions]);
  const userId = currentUser?.id ?? -1;

  const [selfFetched, setSelfFetched] = useState<TileDef[] | null>(tilesProp ? null : null);
  const [userOrder, setUserOrder] = useState<TileOrderMap>({});
  const [filter, setFilter] = useState<CatalogFilter>('available');
  const [query, setQuery] = useState('');

  const tiles = tilesProp ?? selfFetched;

  useEffect(() => {
    if (tilesProp) return;
    let cancelled = false;
    fetch('/api/tiles')
      .then((r) => r.json())
      .then((data: { tiles: any[] }) => {
        if (cancelled) return;
        setSelfFetched((data.tiles ?? []).map((t) => ({
          id: t.id,
          display_name: t.display_name,
          subtitle: t.subtitle,
          icon: t.icon,
          accent: t.accent,
          group: t.group_name as TileGroup,
          sub_view: t.sub_view,
          href: t.href,
          module_id: t.module_id,
          request_target: t.request_target,
          sort_order: t.sort_order,
          requestAccessTarget:
            t.request_target === 'cfo' ? 'cfo'
            : t.request_target === 'admin' ? 'admin'
            : t.request_target === 'hr_manager' ? 'hr_manager'
            : undefined,
          access_meta: { viewPermId: t.view_perm_id },
        })));
      })
      .catch(() => {
        if (!cancelled) setSelfFetched([]);
      });
    return () => { cancelled = true; };
  }, [tilesProp]);

  useEffect(() => {
    setUserOrder(loadOrder(userId));
  }, [userId]);

  const accessMap = useMemo<Record<string, TileAccess>>(() => {
    const out: Record<string, TileAccess> = {};
    if (!tiles) return out;
    for (const t of tiles) {
      const override = accessByTile?.[t.id];
      if (override) { out[t.id] = override; continue; }
      const viewPerm = (t as any).access_meta?.viewPermId ?? `tile:${t.id}:view::allow`;
      const allowed = matchPerm(userPerms, viewPerm);
      out[t.id] = allowed
        ? { state: 'open', reason: 'Allowed by your role.', source: 'perm' }
        : { state: 'locked', reason: 'Restricted by your role.', source: 'perm' };
    }
    return out;
  }, [tiles, userPerms, accessByTile]);

  const annotated: Annotated[] = useMemo(() => {
    if (!tiles) return [];
    return tiles.map((t) => ({ tile: t, access: accessMap[t.id] ?? evaluateTileOptimistic(t, currentUser) }));
  }, [tiles, accessMap, currentUser]);

  const { openCount, lockedCount } = useMemo(() => {
    let open = 0;
    let locked = 0;
    for (const a of annotated) {
      if (a.access.state === 'locked') locked++;
      else open++;
    }
    return { openCount: open, lockedCount: locked };
  }, [annotated]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return annotated.filter(({ tile, access }) => {
      if (filter === 'available' && access.state === 'locked') return false;
      if (filter === 'locked' && access.state !== 'locked') return false;
      if (!q) return true;
      return [tile.display_name, tile.subtitle, tile.id, tile.group]
        .some((value) => String(value ?? '').toLowerCase().includes(q));
    });
  }, [annotated, filter, query]);

  const byGroup = useMemo(() => {
    const map = Object.fromEntries(GROUP_ORDER.map((g) => [g, [] as Annotated[]])) as Record<TileGroup, Annotated[]>;
    for (const it of filtered) map[it.tile.group]?.push(it);
    for (const g of GROUP_ORDER) map[g] = applyOrder(map[g], g, userOrder);
    return map;
  }, [filtered, userOrder]);

  const handleOrderChange = (group: string, _nextIds: string[]) => {
    setUserOrder((prev) => ({ ...prev, [group]: _nextIds }));
  };

  const handleResetGroup = (group: string) => {
    clearGroupOrder(userId, group);
    setUserOrder((prev) => {
      if (!(group in prev)) return prev;
      const next = { ...prev };
      delete next[group];
      return next;
    });
  };

  if (tiles === null || tiles === undefined) {
    return (
      <div className="panel flex items-center justify-center gap-2 p-8 text-center text-xs text-mute">
        <LoaderCircle size={15} className="animate-spin" aria-hidden /> <T id="chrome.loadingTiles" />
      </div>
    );
  }

  return (
    <TileTooltipProvider>
      <div className="mb-8 animate-fade-in space-y-6">
        <div className="panel flex flex-col gap-5 p-4 sm:p-5 xl:flex-row xl:items-center xl:justify-between">
          <div className="flex min-w-0 items-start gap-3">
            <span className="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-accent/25 bg-accent-soft/45 text-accent">
              <LayoutGrid size={18} aria-hidden />
            </span>
            <div className="min-w-0">
              <h2 className="section-title text-ink"><T id="hub.catalogTitle" hideSecondary /></h2>
              <p className="mt-1 text-sm text-mute"><T id="hub.catalogDescription" hideSecondary /></p>
            </div>
          </div>

          <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
            <label className="glass-input flex h-10 min-w-0 items-center gap-2 px-3 sm:w-56">
              <Search size={15} className="shrink-0 text-mute" aria-hidden />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t('hub.searchApps')}
                aria-label={t('hub.searchApps')}
                className="min-w-0 flex-1 bg-transparent text-sm text-ink placeholder:text-mute"
              />
            </label>
            <div className="flex h-10 shrink-0 items-center rounded-xl border border-rule bg-paper/35 p-1" aria-label={t('hub.catalogFilter')}>
              <FilterButton
                active={filter === 'available'}
                icon={CheckCircle2}
                label={<T id="hub.filterAvailable" hideSecondary />}
                ariaLabel={`${t('hub.filterAvailable')} (${openCount})`}
                count={openCount}
                onClick={() => setFilter('available')}
              />
              <FilterButton
                active={filter === 'all'}
                icon={LayoutGrid}
                label={<T id="hub.filterAll" hideSecondary />}
                ariaLabel={`${t('hub.filterAll')} (${annotated.length})`}
                count={annotated.length}
                onClick={() => setFilter('all')}
              />
              <FilterButton
                active={filter === 'locked'}
                icon={Lock}
                label={<T id="hub.filterLocked" hideSecondary />}
                ariaLabel={`${t('hub.filterLocked')} (${lockedCount})`}
                count={lockedCount}
                onClick={() => setFilter('locked')}
              />
            </div>
          </div>
        </div>

        {filtered.length > 0 ? (
          <Section
            byGroup={byGroup}
            userOrder={userOrder}
            activeTileId={activeTileId}
            onSelectTile={onSelectTile}
            actorId={currentUser?.id}
            currentUser={currentUser}
            disableSort={filter !== 'all' || query.trim().length > 0}
            onOrderChange={handleOrderChange}
            onResetGroup={handleResetGroup}
          />
        ) : (
          <div className="panel grid min-h-48 place-items-center p-8 text-center">
            <div>
              <span className="mx-auto inline-flex h-11 w-11 items-center justify-center rounded-full border border-rule bg-paper-2/50 text-mute">
                <Search size={18} aria-hidden />
              </span>
              <p className="mt-3 text-sm font-semibold text-ink"><T id="hub.noApps" hideSecondary /></p>
              <button
                type="button"
                onClick={() => { setQuery(''); setFilter('available'); }}
                className="mt-2 text-xs font-medium text-accent hover:text-accent-strong"
              >
                <T id="hub.clearFilters" hideSecondary />
              </button>
            </div>
          </div>
        )}
      </div>
    </TileTooltipProvider>
  );
};

function FilterButton({
  active,
  icon: Icon,
  label,
  ariaLabel,
  count,
  onClick,
}: {
  active: boolean;
  icon: LucideIcon;
  label: React.ReactNode;
  ariaLabel: string;
  count: number;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={ariaLabel}
      aria-pressed={active}
      className={`inline-flex h-8 items-center gap-1.5 whitespace-nowrap rounded-lg px-2.5 text-xs font-medium transition-colors ${active ? 'bg-paper-3 text-ink shadow-sm' : 'text-mute hover:text-ink'}`}
    >
      <Icon size={13} aria-hidden />
      <span className="hidden sm:inline">{label}</span>
      <span className={`font-mono text-[10px] ${active ? 'text-accent' : 'text-mute'}`}>{count}</span>
    </button>
  );
}

interface SectionProps {
  byGroup: Record<TileGroup, Annotated[]>;
  userOrder: TileOrderMap;
  activeTileId: string;
  onSelectTile: (t: any) => void;
  actorId?: number;
  currentUser: any;
  disableSort: boolean;
  onOrderChange: (group: string, ids: string[]) => void;
  onResetGroup: (group: string) => void;
}

const Section: React.FC<SectionProps> = ({
  byGroup, userOrder, activeTileId, onSelectTile,
  actorId, currentUser, disableSort, onOrderChange, onResetGroup,
}) => {
  return (
    <section className="space-y-8">
      {GROUP_ORDER.map((group) => {
        const items = byGroup[group] ?? [];
        if (items.length === 0) return null;
        const custom = hasGroupOrder(userOrder, group);
        const orderKey = (userOrder[group] ?? []).join('|') || '__default__';
        const GroupIcon = groupIcon(group);
        return (
          <div key={group} className="space-y-3">
            <div className="flex items-center gap-2 border-b border-rule/70 pb-2.5">
              <span className="inline-flex h-7 w-7 items-center justify-center rounded-lg border border-rule bg-paper-2/45 text-mute">
                <GroupIcon size={13} aria-hidden />
              </span>
              <span className="text-xs font-semibold uppercase tracking-[0.13em] text-ink-2">
                <T id={GROUP_LABEL[group].id} hideSecondary />
              </span>
              <span className="font-mono text-[10px] text-mute">{items.length}</span>
              {custom ? (
                <button
                  type="button"
                  onClick={() => onResetGroup(group)}
                  className="ml-auto text-xs font-mono uppercase tracking-widest text-mute hover:text-caution px-2 py-0.5 rounded border border-transparent hover:border-rule transition-colors"
                  title="Reset to default order (open tiles left, locked tiles right)"
                >
                  <T id="hub.resetOrder" hideSecondary />
                </button>
              ) : null}
            </div>
            <SortableTileGrid
              key={`${group}:${orderKey}`}
              group={group}
              items={items}
              activeTileId={activeTileId}
              onSelectTile={onSelectTile}
              actorId={actorId}
              targetLabel={targetLabel}
              currentUser={currentUser}
              disabled={disableSort}
              onOrderChange={onOrderChange}
            />
          </div>
        );
      })}
    </section>
  );
};

export default TileHub;
