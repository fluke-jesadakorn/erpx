import 'server-only';
import { headers } from 'next/headers';
import { notFound, redirect } from 'next/navigation';
import { BookMarked } from 'lucide-react';
import { loadWaybillRailContext } from '@/waybill/queries';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { loadJournalForWaybill } from '@/waybill/queries';
import { WaybillGlSection } from '@/components/waybill/WaybillGlSection';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { crumbsForPath } from '@/components/breadcrumbs/routes';
import { getSecondaryLocale } from '@/server/locale';
import { OverviewShell } from '../_components/Overview';
import { Empty } from '@/components/ui';
import { authorizeExpenseStage, loadExpenseFlowContext, type ExpenseActor } from '@/waybill/expenseFlow';
import { query } from '@/db';
import { recomputeExpenseDraftGlAction } from '@/app/actions/waybill';

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
  let canFinalApprove = false;
  let canConfirmGl = false;
  let canEditDraft = false;
  if (wb.origin === 'expense' && !['completed', 'rejected'].includes(stage)) {
    const flow = await loadExpenseFlowContext(wb.id);
    const flowActor: ExpenseActor = {
      id: actor.id,
      permissions: actor.permissions,
      deptId: actor.dept_id,
      departmentId: actor.dept_id,
      level: actor.level,
      rank: actor.level,
      roleName: actor.role_name,
    };
    const decision = await authorizeExpenseStage(flowActor, flow);
    const claim = await query<{ claimed_by: number }>(
      `SELECT claimed_by FROM waybill_stage_claims
        WHERE waybill_id = $1 AND stage = $2 AND released_at IS NULL`,
      [wb.id, stage],
    );
    const ownsClaim = claim.rows[0]?.claimed_by === actor.id;
    canFinalApprove = stage === 'accounting_approval' && decision.allow;
    canConfirmGl = stage === 'settlement' && decision.allow && ownsClaim;
    canEditDraft = ['accounting_review', 'settlement'].includes(stage) && decision.allow && ownsClaim;
  }

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
              canEditDraft={canEditDraft}
              isFinalApproval={stage === 'accounting_approval'}
              isDisbursed={stage === 'settlement'}
            />
          ) : (
            <div className="space-y-3">
              <Empty icon={BookMarked} title="No GL journal available for this waybill yet." />
              {canEditDraft && (
                <form action={recomputeExpenseDraftGlAction}>
                  <input type="hidden" name="waybillId" value={wb.id} />
                  <button type="submit" className="rounded-md bg-info px-3 py-2 text-sm font-semibold text-paper">
                    Ask AI to draft GL
                  </button>
                </form>
              )}
            </div>
          )}
        </OverviewShell>
      </PageLayout>
    </>
  );
}
