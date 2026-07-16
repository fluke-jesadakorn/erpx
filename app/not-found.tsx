import { loadActor } from '@folio-lib/server/guard';
import { NoPermissionView } from '@/components/NoPermissionView';

export const dynamic = 'force-dynamic';

export default async function NotFound({ searchParams }: { searchParams?: Promise<Record<string, string | string[] | undefined>> } = {}) {
  const actor = await loadActor();
  const sp = searchParams ? await searchParams : undefined;
  const attemptedPath = typeof sp?.['path'] === 'string' ? sp['path'] : undefined;

  return (
    <NoPermissionView
      kind="not_found"
      actor={actor as any}
      attemptedPath={attemptedPath}
      reason="The URL you visited does not exist or has been moved."
    />
  );
}