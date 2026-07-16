'use client';

import React, { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { CommandPalette } from '@/components/CommandPalette';
import { TileHub } from '@/components/TileHub';
import { HubHero } from '@/components/HubHero';
import { AccessDenied } from '@/components/AccessDenied';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { type TileDef, tileHref, tileFromRow } from '@/components/tile-config';
import { evaluateTileOptimistic, type TileAccess } from '@/components/tileAccess';
import { ROOT_CRUMB } from '@/components/breadcrumbs';
import type { GreetingKey } from '@/hero';
import { T } from '@/components/i18n/T';

interface HomeClientProps {
  users: any[];
  currentUser: any | null;
  expenses: any[];
  policies?: any[];
  prs: any[];
  execReport: any | null;
  canViewHub: boolean;
  tiles?: TileDef[];
  accessByTile?: Record<string, TileAccess>;
  greetingKey?: GreetingKey;
}

export function HomeClient({ users, currentUser, expenses: _expenses, policies: _policies, prs, execReport: _execReport, canViewHub, greetingKey }: HomeClientProps) {
  const router = useRouter();

  const [openCommand, setOpenCommand] = useState(false);
  const [tiles, setTiles] = useState<TileDef[]>([]);
  const [tilesLoaded, setTilesLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    fetch('/api/tiles')
      .then((r) => r.json())
      .then((data: { tiles: any[] }) => {
        if (cancelled) return;
        setTiles((data.tiles ?? []).map((t) => tileFromRow(t)));
      })
      .catch(() => {
        if (!cancelled) setTiles([]);
      })
      .finally(() => { if (!cancelled) setTilesLoaded(true); });
    return () => { cancelled = true; };
  }, []);

  const visibleTiles = tiles;

  const handleSelectTile = (t: any) => {
    const access = evaluateTileOptimistic(t, currentUser);
    if (access.state === 'open' || access.state === 'checking') router.push(tileHref(t.id));
  };

  const handleOpenCommand = () => setOpenCommand(true);

  if (currentUser && !canViewHub) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB]} />
        <PageLayout
          title={<T id="nav.home" />}
          subtitle={<T id="chrome.restricted" />}
        >
          <AccessDenied roleName={currentUser.role_name} requiredAccess="Hub" />
        </PageLayout>
      </>
    );
  }

  return (
    <>
      <BreadcrumbSetter crumbs={[ROOT_CRUMB]} />
      <CommandPalette
        role={currentUser?.role_name as any}
        onNavigate={(href) => router.push(href)}
        users={users}
        currentUser={currentUser}
        openCommand={openCommand}
        setOpenCommand={setOpenCommand}
        tiles={tiles}
      />

      <PageLayout
        title={<T id="nav.home" />}
        subtitle={currentUser ? <T id="hub.catalogSubtitle" values={{ n: tiles.length }} /> : <T id="chrome.signInRequired" />}
      >
        {currentUser && tilesLoaded && visibleTiles.length > 0 && (
          <div className="mb-8">
            <HubHero
              actor={currentUser}
              tiles={visibleTiles as any}
              pendingPrs={prs as any[]}
              initialGreetingKey={greetingKey}
              isLocked={(t) => evaluateTileOptimistic(t, currentUser).state === 'locked'}
              onOpenCommand={handleOpenCommand}
            />
          </div>
        )}

        {!currentUser && (
          <div className="flex justify-center items-center py-10 glass-panel rounded-2xl border-indigo-500/20">
            <span className="ml-3 text-xs font-mono text-slate-300">
               <T id="chrome.noUserSession" />
            </span>
          </div>
        )}

        {currentUser && tilesLoaded && visibleTiles.length > 0 && (
          <TileHub
            currentUser={currentUser}
            tiles={visibleTiles}
            activeTileId=""
            onSelectTile={handleSelectTile}
          />
        )}

{currentUser && tilesLoaded && visibleTiles.length === 0 && (
          <div className="flex justify-center items-center py-10 glass-panel rounded-2xl border border-slate-800">
             <span className="text-sm text-slate-400 font-sans"><T id="chrome.noTiles" /></span>
          </div>
        )}
      </PageLayout>
    </>
  );
}