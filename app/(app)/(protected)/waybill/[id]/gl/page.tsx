import 'server-only';
import Link from 'next/link';
import { headers } from 'next/headers';
import { notFound, redirect } from 'next/navigation';
import { ArrowRight, BookMarked, CircleCheckBig } from 'lucide-react';
import { loadWaybillRailContext } from '@/waybill/queries';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { loadJournalForWaybill } from '@/waybill/queries';
import { WaybillGlSection } from '@/components/waybill/WaybillGlSection';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { crumbsForPath } from '@/components/breadcrumbs/routes';
import { getSecondaryLocale } from '@/server/locale';
import { T } from '@/components/i18n/TServer';
import { OverviewShell } from '../_components/Overview';
import { Empty } from '@/components/ui';
import { authorizeExpenseStage, loadExpenseFlowContext, type ExpenseActor } from '@/waybill/expenseFlow';
import { query } from '@/db';
import { recomputeExpenseDraftGlAction } from '@/app/actions/waybill';
import { GlSubmit } from '@/components/waybill/GlSubmit';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ id: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}

export default async function WaybillGlPage({ params, searchParams }: PageProps) {
  const { id } = await params;
  const sp = await searchParams;
  const notice = Array.isArray(sp.notice) ? sp.notice[0] : sp.notice;
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
    canEditDraft = stage === 'accounting_review' && decision.allow && ownsClaim;
  }

  const journal = await loadJournalForWaybill(wb.id);

  return (
    <>
      <BreadcrumbSetter
        crumbs={crumbsForPath(`/waybill/${wb.id}/gl`, locale, { waybillId: wb.id, subtab: 'gl' })}
      />
      <PageLayout title={`GL · ${wb.id}`} subtitle={journal ? `${journal.kind} journal` : 'No journal'}>
        <OverviewShell waybillId={wb.id} active="gl">
          {notice === 'missing-accrual-draft' && (
            <p className="mb-4 rounded-md border border-caution bg-caution-soft p-3 text-sm text-caution-strong" role="alert">
              <T id="waybill.gl.missingAccrualDraft" locale={locale} />
            </p>
          )}
          {(notice === 'ai-draft-ready' || notice === 'draft-saved') && (
            <section className="mb-4 flex flex-wrap items-center gap-3 rounded-md border border-positive/50 bg-positive-soft p-4" role="status" aria-live="polite">
              <CircleCheckBig className="size-5 shrink-0 text-positive" aria-hidden />
              <div className="min-w-0 flex-1">
                <T
                  id={notice === 'ai-draft-ready' ? 'waybill.gl.aiDraftReady' : 'waybill.gl.draftSaved'}
                  variant="stacked"
                  locale={locale}
                  primaryClassName="block text-sm font-semibold text-positive"
                  secondaryClassName="mt-0.5 block text-xs font-normal text-positive/80"
                />
                <p className="mt-1 text-sm text-ink-2">
                  <T
                    id={notice === 'ai-draft-ready' ? 'waybill.gl.aiDraftReadyHint' : 'waybill.gl.draftSavedNext'}
                    variant="stacked"
                    locale={locale}
                    primaryClassName="block font-normal leading-relaxed text-ink-2"
                    secondaryClassName="mt-0.5 block text-xs font-normal leading-relaxed text-mute"
                  />
                </p>
              </div>
              {notice === 'draft-saved' && (
                <Link href={`/waybill/${wb.id}#waybill-task`} className="inline-flex min-h-10 items-center gap-2 rounded-md bg-positive px-3 py-2 text-sm font-semibold text-paper transition hover:bg-positive-strong">
                  <T id="waybill.gl.continueReview" locale={locale} variant="compact" />
                  <ArrowRight className="size-4 shrink-0" aria-hidden />
                </Link>
              )}
            </section>
          )}
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
                  <GlSubmit
                    label="waybill.gl.askAiDraft"
                    pendingLabel="waybill.gl.aiDrafting"
                    pendingHint="waybill.gl.aiDraftingHint"
                    icon="✦"
                    testId={`gl-ai-draft-empty-${wb.id}`}
                    className="rounded-md bg-info px-3 py-2 text-sm font-semibold text-paper hover:bg-info-strong"
                  />
                </form>
              )}
            </div>
          )}
        </OverviewShell>
      </PageLayout>
    </>
  );
}
