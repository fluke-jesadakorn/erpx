import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { PlatformConsole } from '@/components/platform/PlatformConsole';
import { FolioBridgeReadStatus } from '@/components/platform/FolioBridgeReadStatus';
import { getSecondaryLocale } from '@/server/locale';
import { loadActor } from '@/server/guard';
import { canAccessPlatform } from '@/platform/access';
import { loadPlatformOverview } from '@/platform/queries';
import { loadFolioBridgeReadStatus } from '@/platform/bridge';
import { T } from '@/components/i18n/TServer';

export const dynamic = 'force-dynamic';

export default async function PlatformPage() {
  const locale = await getSecondaryLocale();
  const actor = await loadActor();
  if (!actor || !canAccessPlatform(actor)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: <T id="breadcrumbs.home" locale={locale} />, href: '/' }, { label: 'ERPX Platform' }]} />
        <PageLayout title="ERPX Platform" subtitle="Customer operations and support control plane">
          <NoPermissionView kind="locked" actor={actor as any} attemptedPath="/platform" reason="Platform Customer Console access is restricted to Platform Administrators." />
        </PageLayout>
      </>
    );
  }

  let overview = null;
  let schemaReady = true;
  const bridgeStatus = await loadFolioBridgeReadStatus();
  try {
    overview = await loadPlatformOverview();
  } catch (error) {
    schemaReady = false;
    console.error('ERPX platform control plane is unavailable:', error);
  }

  return (
    <>
      <BreadcrumbSetter crumbs={[{ label: <T id="breadcrumbs.home" locale={locale} />, href: '/' }, { label: 'ERPX Platform' }]} />
      <PageLayout
        width="full"
        title="ERPX Platform"
        subtitle={`Customer 360 · support access · expiring RBAC · global read gateway · ${actor.fullname}`}
      >
        <FolioBridgeReadStatus status={bridgeStatus} />
        <PlatformConsole initialOverview={overview} schemaReady={schemaReady} />
      </PageLayout>
    </>
  );
}
