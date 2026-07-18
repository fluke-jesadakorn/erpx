import 'server-only';
import { headers } from 'next/headers';
import { notFound, redirect } from 'next/navigation';
import { BookMarked } from 'lucide-react';
import { loadWaybillRailContext } from '@/waybill/queries';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
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
  const h = await headers();
  const session = await loadActivePermSession(
    new Request(`http://internal/waybill/${id}/gl`, { headers: h as unknown as HeadersInit }),
  );
  if (!session) redirect('/login');

  const ctx = await loadWaybillRailContext(id);
  if (!ctx) notFound();
  const wb = ctx.waybill;
  const locale = await getSecondaryLocale();

  const actorCanSeeGlLines = hasPermission(session.session, 'finance:gl:view::allow');
  const stage = wb.current_stage;
  const canFinalApprove = stage === 'accounting_review'
    && hasPermission(session.session, PERM.finance.expense.approve);
  const canConfirmGl = stage === 'disbursed'
    && hasPermission(session.session, 'finance:gl:confirm::allow');

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
            <Empty icon={BookMarked} title="No GL journal available for this waybill yet." />
          )}
        </OverviewShell>
      </PageLayout>
    </>
  );
}