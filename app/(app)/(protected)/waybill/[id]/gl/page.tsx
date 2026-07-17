import 'server-only';
import { notFound, redirect } from 'next/navigation';
import { loadWaybillRailContext } from '@/waybill/queries';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/server';
import { loadJournalForWaybill } from '@/waybill/queries';
import { WaybillGlSection } from '@/components/waybill/WaybillGlSection';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { crumbsForPath } from '@/components/breadcrumbs/routes';
import { getSecondaryLocale } from '@/server/locale';
import { OverviewShell } from '../_components/Overview';
import { Empty } from '@/components/ui';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ id: string }>;
}

export default async function WaybillGlPage({ params }: PageProps) {
  const { id } = await params;
  const actor = await loadActor();
  if (!actor) redirect('/login');

  const ctx = await loadWaybillRailContext(id);
  if (!ctx) notFound();
  const wb = ctx.waybill;
  const locale = await getSecondaryLocale();

  const perms = actor.permissions;
  const actorCanSeeGlLines = matchPerm(perms, 'finance:gl:view::allow');
  const stage = wb.current_stage;
  const canFinalApprove = stage === 'accounting_review'
    && matchPerm(perms, 'finance:expense:approve::allow');
  const canConfirmGl = stage === 'disbursed'
    && matchPerm(perms, 'finance:gl:confirm::allow');

  const journal = await loadJournalForWaybill(wb.id);

  return (
    <>
      <BreadcrumbSetter
        crumbs={crumbsForPath(`/waybill/${wb.id}/gl`, locale, { waybillId: wb.id, subtab: 'gl' })}
      />
      <PageLayout title={`GL · ${wb.id}`} subtitle={journal ? `${journal.kind} journal` : 'No journal'}>
        <OverviewShell waybillId={wb.id} active="gl">
          {journal ? (
            <WaybillGlSection
              waybillId={wb.id}
              origin={wb.origin as 'expense' | 'pr' | 'po' | 'so'}
              journal={journal}
              actorRole={actor.role_name}
              actorCanSeeLines={actorCanSeeGlLines}
              lang={locale}
              canFinalApprove={canFinalApprove}
              canConfirmGl={canConfirmGl}
              isFinalApproval={stage === 'accounting_review'}
              isDisbursed={stage === 'disbursed'}
            />
          ) : (
            <Empty icon="wb-ledger" title="No GL journal available for this waybill yet." />
          )}
        </OverviewShell>
      </PageLayout>
    </>
  );
}