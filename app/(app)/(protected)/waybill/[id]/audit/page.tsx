import 'server-only';
import { notFound, redirect } from 'next/navigation';
import { loadWaybillEvents, loadWaybillRailContext } from '@/waybill/queries';
import { loadActor } from '@/server/guard';
import { verifyEventChain } from '@/waybill/events';
import { WaybillAuditSection } from '@/components/waybill/WaybillAuditSection';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { crumbsForPath } from '@/components/breadcrumbs/routes';
import { getSecondaryLocale } from '@/server/locale';
import { OverviewShell } from '../_components/Overview';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ id: string }>;
}

export default async function WaybillAuditPage({ params }: PageProps) {
  const { id } = await params;
  const actor = await loadActor();
  if (!actor) redirect('/login');

  const ctx = await loadWaybillRailContext(id);
  if (!ctx) notFound();
  const wb = ctx.waybill;
  const locale = await getSecondaryLocale();

  const [events, integrity] = await Promise.all([
    loadWaybillEvents(wb.id),
    verifyEventChain(wb.id),
  ]);

  return (
    <>
      <BreadcrumbSetter
        crumbs={crumbsForPath(`/waybill/${wb.id}/audit`, locale, { waybillId: wb.id, subtab: 'audit' })}
      />
      <PageLayout title={`Audit · ${wb.id}`} subtitle={`${events.length} events`}>
        <OverviewShell waybillId={wb.id} active="audit">
          <WaybillAuditSection
            waybillId={wb.id}
            events={events}
            integrity={integrity}
            locale={locale}
          />
        </OverviewShell>
      </PageLayout>
    </>
  );
}