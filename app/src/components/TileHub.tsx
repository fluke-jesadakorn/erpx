'use client';

import React, { useEffect, useMemo, useState } from 'react';
import { TileTooltipProvider } from './TileTooltip';
import { SortableTileGrid } from './tiles/SortableTileGrid';
import {
  type TileDef,
  type TileWithMeta,
  type TileGroup,
  GROUP_LABEL,
  GROUP_ORDER,
  targetLabel,
} from './tile-config';
import {
  evaluateTileOptimistic,
  tileAccessFromBatchResult,
  type TileAccess,
} from './tileAccess';
import { applyOrder, clearGroupOrder, hasGroupOrder, loadOrder, type TileOrderMap } from '@/lib/tileOrder';
import { matchPerm } from '@folio-lib/perm';

interface TileHubProps {
  currentUser: any;
  tiles?: TileDef[];
  activeTileId: string;
  onSelectTile: (t: any) => void;
  accessByTile?: Record<string, TileAccess>;
}

type Annotated = { tile: TileWithMeta; access: TileAccess };

export const TileHub: React.FC<TileHubProps> = ({
  currentUser,
  tiles: tilesProp,
  activeTileId,
  onSelectTile,
  accessByTile,
}) => {
  const userPerms: string[] = currentUser?.permissions ?? [];
  const userId = currentUser?.id ?? -1;

  const [selfFetched, setSelfFetched] = useState<TileDef[] | null>(tilesProp ? null : null);
  const [userOrder, setUserOrder] = useState<TileOrderMap>({});

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

  // Resolve access synchronously from the actor's permission list.
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

  const byGroup = useMemo(() => {
    const map = Object.fromEntries(GROUP_ORDER.map((g) => [g, [] as Annotated[]])) as Record<TileGroup, Annotated[]>;
    for (const it of annotated) map[it.tile.group]?.push(it);
    for (const g of GROUP_ORDER) map[g] = applyOrder(map[g], g, userOrder);
    return map;
  }, [annotated, userOrder]);

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
      <div className="glass-panel rounded-2xl border border-slate-800 p-8 text-center text-slate-500 font-mono text-xs">
        ⏳ Loading tiles…
      </div>
    );
  }

  return (
    <TileTooltipProvider>
      <div className="mb-8 animate-fade-in space-y-6">
        <div className="flex items-center gap-3 flex-wrap">
          <span className="text-base">🗂</span>
          <h3 className="text-sm font-black uppercase tracking-wider text-slate-100">Your Tiles</h3>
          <span className="text-xs font-mono text-slate-500">
            {openCount} accessible · {lockedCount} locked · request access
          </span>
        </div>
        <Section
          byGroup={byGroup}
          userOrder={userOrder}
          activeTileId={activeTileId}
          onSelectTile={onSelectTile}
          actorId={currentUser?.id}
          currentUser={currentUser}
          onOrderChange={handleOrderChange}
          onResetGroup={handleResetGroup}
        />
      </div>
    </TileTooltipProvider>
  );
};

interface SectionProps {
  byGroup: Record<TileGroup, Annotated[]>;
  userOrder: TileOrderMap;
  activeTileId: string;
  onSelectTile: (t: any) => void;
  actorId?: number;
  currentUser: any;
  onOrderChange: (group: string, ids: string[]) => void;
  onResetGroup: (group: string) => void;
}

const Section: React.FC<SectionProps> = ({
  byGroup, userOrder, activeTileId, onSelectTile,
  actorId, currentUser, onOrderChange, onResetGroup,
}) => {
  return (
    <section className="rounded-2xl border border-slate-800 bg-gradient-to-br from-slate-950/60 to-slate-950 p-4 space-y-4">
      {GROUP_ORDER.map((group) => {
        const items = byGroup[group] ?? [];
        if (items.length === 0) return null;
        const custom = hasGroupOrder(userOrder, group);
        const orderKey = (userOrder[group] ?? []).join('|') || '__default__';
        return (
          <div key={group} className="space-y-2">
            <div className="flex items-center gap-1.5">
              <span aria-hidden className="text-sm">{GROUP_LABEL[group].icon}</span>
              <span className="text-xs font-mono font-bold uppercase tracking-widest text-slate-500">
                {GROUP_LABEL[group].label}
              </span>
              {custom ? (
                <button
                  type="button"
                  onClick={() => onResetGroup(group)}
                  className="ml-auto text-xs font-mono uppercase tracking-widest text-slate-500 hover:text-amber-400 px-2 py-0.5 rounded border border-transparent hover:border-slate-700 transition-colors"
                  title="Reset to default order (open tiles left, locked tiles right)"
                >
                  ↺ reset
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
              onOrderChange={onOrderChange}
            />
          </div>
        );
      })}
    </section>
  );
};

export default TileHub;