import { loadActor } from '@folio-lib/server/guard';
import { NoPermissionView } from '@/components/NoPermissionView';

export const dynamic = 'force-dynamic';

interface Props {
  searchParams?: Promise<Record<string, string | string[] | undefined>>;
}

export default async function ForbiddenPage({ searchParams }: Props = {}) {
  const actor = await loadActor();
  const sp = searchParams ? await searchParams : undefined;
  const attemptedPath = typeof sp?.['path'] === 'string' ? sp['path'] : undefined;
  const reason = typeof sp?.['reason'] === 'string' ? sp['reason'] : undefined;

  return (
    <NoPermissionView
      kind="locked"
      actor={actor as any}
      attemptedPath={attemptedPath}
      reason={reason ?? 'You do not have permission to access this page.'}
    />
  );
}
