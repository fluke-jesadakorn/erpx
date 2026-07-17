import 'server-only';
import { notFound, redirect } from 'next/navigation';
import { loadWaybillRailContext } from '@/waybill/queries';
import { loadActor } from '@/server/guard';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { crumbsForPath } from '@/components/breadcrumbs/routes';
import { getSecondaryLocale } from '@/server/locale';
import { OverviewShell } from '../_components/Overview';
import { WaybillChat } from '../_components/WaybillChat';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ id: string }>;
}

export default async function WaybillChatPage({ params }: PageProps) {
  const { id } = await params;
  const actor = await loadActor();
  if (!actor) redirect('/login');

  const ctx = await loadWaybillRailContext(id);
  if (!ctx) notFound();
  const wb = ctx.waybill;
  const locale = await getSecondaryLocale();

  return (
    <>
      <BreadcrumbSetter
        crumbs={crumbsForPath(`/waybill/${wb.id}/chat`, locale, { waybillId: wb.id, subtab: 'chat' })}
      />
      <PageLayout title={`Chat · ${wb.id}`} subtitle={`Per-waybill thread scoped to ${wb.id}`}>
        <OverviewShell waybillId={wb.id} active="chat">
          <WaybillChat waybillId={wb.id} />
        </OverviewShell>
      </PageLayout>
    </>
  );
}