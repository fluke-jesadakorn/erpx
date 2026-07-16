import { Suspense } from 'react';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/server';
import { SignInPanel } from '@/components/SignInPanel';
import { HomeTilesFetcher } from './(protected)/_components/HomeTilesFetcher';
import { HomeTilesFallback } from './(protected)/_components/HomeTilesFallback';

export const dynamic = 'force-dynamic';

export default async function TilesPage() {
  const actor = await loadActor();
  if (!actor) {
    return <SignInPanel />;
  }

  const perms = actor.permissions ?? [];
  const canViewPolicy = matchPerm(perms, 'rbac:matrix:view::allow');
  const canViewExec = matchPerm(perms, 'tile:cockpit:view::allow');
  const canViewHub = true;

  return (
    <Suspense fallback={<HomeTilesFallback />}>
      <HomeTilesFetcher
        actor={actor as any}
        canViewHub={canViewHub}
        canViewPolicy={canViewPolicy}
        canViewExec={canViewExec}
      />
    </Suspense>
  );
}