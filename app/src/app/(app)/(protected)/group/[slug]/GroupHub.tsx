'use client';

import React, { useState } from 'react';
import { useRouter } from 'next/navigation';
import { CommandPalette } from '@/components/CommandPalette';
import { TileHub } from '@/components/TileHub';
import { HubHero } from '@/components/HubHero';
import { AccessDenied } from '@/components/AccessDenied';
import { PageLayout } from '@/components/PageLayout';
import { GROUP_LABEL, type TileDef, type TileGroup, tileHref } from '@/components/tile-config';
import { evaluateTileOptimistic } from '@/components/tileAccess';
import type { TileAccess } from '@/components/tileAccess';
import type { GreetingKey } from '@/lib/hero';

interface Props {
  actor: any;
  group: TileGroup;
  users: any[];
  prs: any[];
  tiles: TileDef[];
  accessByTile: Record<string, TileAccess>;
  greetingKey: GreetingKey;
}

export function GroupHub({ actor, group, users, prs, tiles, accessByTile, greetingKey }: Props) {
  const router = useRouter();
  const [openCommand, setOpenCommand] = useState(false);

  const isLocked = (t: TileDef) =>
    (accessByTile[t.id] ?? evaluateTileOptimistic(t, actor)).state === 'locked';

  const title = GROUP_LABEL[group].label.toUpperCase();
  const subtitle = `${tiles.length} tile${tiles.length === 1 ? '' : 's'} in ${GROUP_LABEL[group].label}`;

  const handleSelectTile = (t: any) => {
    const access = accessByTile[t.id] ?? evaluateTileOptimistic(t, actor);
    if (access.state === 'open' || access.state === 'checking') router.push(tileHref(t.id));
  };

  if (actor && actor.role_name) {
    return (
      <PageLayout title={title} subtitle="Restricted">
        <AccessDenied roleName={actor.role_name} requiredAccess={`${GROUP_LABEL[group].label} group`} />
      </PageLayout>
    );
  }

  return (
    <>
      <CommandPalette
        role={actor?.role_name as any}
        onNavigate={(href) => router.push(href)}
        users={users}
        currentUser={actor}
        openCommand={openCommand}
        setOpenCommand={setOpenCommand}
        tiles={tiles}
      />

      <PageLayout title={title} subtitle={subtitle}>
        {actor && tiles.length > 0 && (
          <div className="mb-8">
            <HubHero
              actor={actor}
              tiles={tiles as any}
              pendingPrs={prs}
              isLocked={isLocked}
              onOpenCommand={() => setOpenCommand(true)}
              initialGreetingKey={greetingKey}
            />
          </div>
        )}

        {actor && tiles.length > 0 && (
          <TileHub
            currentUser={actor}
            tiles={tiles}
            activeTileId=""
            onSelectTile={handleSelectTile}
            accessByTile={accessByTile}
          />
        )}

        {actor && tiles.length === 0 && (
          <div className="flex justify-center items-center py-10 glass-panel rounded-2xl border-slate-800">
            <span className="text-sm text-slate-400 font-sans">
              No tiles in {GROUP_LABEL[group].label}.
            </span>
          </div>
        )}
      </PageLayout>
    </>
  );
}

export default GroupHub;
