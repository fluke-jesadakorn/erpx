import 'server-only';
import { getDashboardData, getExecutiveReport } from '@/dashboard/queries';
import { loadActionQueueSummary } from '@/notifications/queries';
import { HomeClient } from '@/app/(app)/HomeClient';
import {
  loadActorAsSession,
  loadTileAccessBundle,
} from '@/components/tileAccess.server';
import { timeGreeting } from '@/hero';

interface Props {
  actor: {
    id: number;
    role_name: string;
    role_id?: string | null;
    fullname?: string;
    permissions?: string[] | null;
    [k: string]: unknown;
  };
  canViewHub: boolean;
  canViewExec: boolean;
}

export async function HomeTilesFetcher({ actor, canViewHub, canViewExec }: Props) {
  const [{ users = [], expenses = [] }, actionQueue, execRes] = await Promise.all([
    getDashboardData(),
    loadActionQueueSummary(actor.id, 3),
    canViewExec ? getExecutiveReport(actor.id) : Promise.resolve({ success: false as const, report: null }),
  ]);

  const permSession = await loadActorAsSession();
  const [bundle, greetingKey] = await Promise.all([
    loadTileAccessBundle(permSession),
    Promise.resolve(timeGreeting()),
  ]);

  // Global override: any signed-in user opens the Hub.
  const globalOpen = (actor.permissions ?? []).some((p) =>
    p === 'system:authenticated:view::allow' || p === 'admin:system:bypass::allow',
  );

  return (
    <HomeClient
      users={users as any[]}
      currentUser={actor as any}
      expenses={expenses as any[]}
      actionQueue={actionQueue}
      execReport={execRes.success ? execRes.report : null}
      canViewHub={canViewHub || globalOpen}
      tiles={bundle.tiles}
      accessByTile={bundle.accessByTile}
      greetingKey={greetingKey}
    />
  );
}
