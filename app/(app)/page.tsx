import { Suspense } from 'react';
import { headers } from 'next/headers';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { SignInPanel } from '@/components/SignInPanel';
import { HomeTilesFetcher } from './(protected)/_components/HomeTilesFetcher';
import { HomeTilesFallback } from './(protected)/_components/HomeTilesFallback';

export const dynamic = 'force-dynamic';

export default async function TilesPage() {
  const h = await headers();
  const session = await loadActivePermSession(
    new Request('http://internal', { headers: h as unknown as HeadersInit }),
  );
  const actor = await loadActor();
  if (!session || !actor) {
    return <SignInPanel />;
  }

  const canViewExec = hasPermission(session.session, PERM.tile.cockpit.view);
  const canViewHub = true;

  return (
    <Suspense fallback={<HomeTilesFallback />}>
      <HomeTilesFetcher
        actor={actor as any}
        canViewHub={canViewHub}
        canViewExec={canViewExec}
      />
    </Suspense>
  );
}