import { loadActor } from '@/server/guard';
import { NoPermissionView } from '@/components/NoPermissionView';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

interface Props {
  searchParams?: Promise<Record<string, string | string[] | undefined>>;
}

export default async function ForbiddenPage({ searchParams }: Props = {}) {
  const actor = await loadActor();
  const sp = searchParams ? await searchParams : undefined;
  const attemptedPath = typeof sp?.['path'] === 'string' ? sp['path'] : undefined;
  const reason = typeof sp?.['reason'] === 'string' ? sp['reason'] : undefined;
  const locale = await getSecondaryLocale();

  return (
    <NoPermissionView
      kind="locked"
      actor={actor as any}
      attemptedPath={attemptedPath}
       reason={reason ?? <T id="access.deniedBody" locale={locale} />}
    />
  );
}
