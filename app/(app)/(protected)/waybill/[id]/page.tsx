import React, { Suspense } from 'react';
import Link from 'next/link';
import { headers } from 'next/headers';
import { notFound, redirect } from 'next/navigation';
import { CircleAlert } from 'lucide-react';
import {
  loadWaybillRailContext,
  loadApproversByStage,
  loadActedUsersByStage,
  loadExpenseFullPicture,
  loadWaybillEvents,
} from '@/waybill/queries';
import { loadActor } from '@/server/guard';
import { query } from '@/db';
import { loadVisionModels } from '@/ai/loadVisionModels';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { getSecondaryLocale } from '@/server/locale';
import { pipsForDomain, pipIndex, domainForOrigin } from '@/waybill/derive';
import { verifyEventChain } from '@/waybill/events';
import { WaybillStepCards } from '@/components/waybill/WaybillStepCards';
import { WaybillTimelineBigPicture } from '@/components/waybill/WaybillTimelineBigPicture';
import { WaybillAuditSection } from '@/components/waybill/WaybillAuditSection';
import { WaybillExpenseCollapsible } from '@/components/waybill/WaybillExpenseCollapsible';
import { WaybillHeader } from '@/components/waybill/WaybillHeader';
import { WaybillRiskBadge } from '@/components/waybill/WaybillRiskBadge';
import { WaybillAnomalyBadge } from '@/components/waybill/WaybillAnomalyBadge';
import { WaybillReviewHint } from '@/components/waybill/WaybillReviewHint';
import { InlineActionForm } from '@/components/waybill/InlineActionForm';
import { ExportPdfButton } from '@/components/waybill/ExportPdfButton';
import { DecisionBar } from '@/components/waybill/DecisionBar';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { T } from '@/components/i18n/TServer';
import { authorizeExpenseStage, loadExpenseFlowContext, type ExpenseActor } from '@/waybill/expenseFlow';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ id: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}

function asString(v: string | string[] | undefined): string | null {
  if (Array.isArray(v)) return v[0] ?? null;
  return v ?? null;
}

function isRejectedStage(stage: string): boolean {
  return stage === 'rejected';
}

export default async function WaybillDetail({ params, searchParams }: PageProps) {
  const { id } = await params;
  const sp = await searchParams;

  const actor = await loadActor();
  if (!actor) redirect('/login');
  const h = await headers();
  const session = await loadActivePermSession(
    new Request(`http://internal/waybill/${id}`, { headers: h as unknown as HeadersInit }),
  );
  if (!session) redirect('/login');

  const ctx = await loadWaybillRailContext(id);
  if (!ctx) notFound();

  const wb = ctx.waybill;
  if (wb.origin === 'so') redirect(`/sales/${wb.origin_id}`);

  const action = asString(sp.action);

  const [approversByStage, actedUsersByStage, visionModels, locale, expensePicture, events, integrity, openClaim] = await Promise.all([
    loadApproversByStage(wb.id),
    loadActedUsersByStage(wb.id),
    loadVisionModels(),
    getSecondaryLocale(),
    wb.origin === 'expense' ? loadExpenseFullPicture(wb.origin_id) : Promise.resolve(null),
    loadWaybillEvents(wb.id),
    verifyEventChain(wb.id),
    wb.origin === 'expense'
      ? query<{ claimed_by: number; fullname: string }>(
          `SELECT c.claimed_by, u.fullname
             FROM waybill_stage_claims c JOIN users u ON u.id = c.claimed_by
            WHERE c.waybill_id = $1 AND c.stage = $2 AND c.released_at IS NULL`,
          [wb.id, wb.current_stage],
        ).then((res) => res.rows[0] ?? null)
      : Promise.resolve(null),
  ]);

  const stage = wb.current_stage;
  const perms = session.session.permissions;
  const actorCanSeeGlLines = hasPermission(session.session, 'finance:gl:view::allow');
  const isCrossDeptUiStage = [
    'accounting_verification',
    'accounting_supervision',
    'accounting_authorization',
    'disbursement_authorization',
    'cfo_authorization',
    'ceo_authorization',
  ].includes(stage);
  const isDeptScopedUiStage = [
    'submission',
    'dept_verification',
    'dept_authorization',
    'final_authorization',
  ].includes(stage);
  let sameDeptAsSubmitter = true;
  if (isDeptScopedUiStage && wb.submitter_id) {
    const submitterDeptRes = await query<{ department_id: string | null }>(
      `SELECT department_id FROM perm.user_departments WHERE user_id = $1`,
      [wb.submitter_id],
    );
    const submitterDept = submitterDeptRes.rows[0]?.department_id ?? null;
    sameDeptAsSubmitter = !!actor.dept_id && actor.dept_id === submitterDept;
  }
  const hasStageAll = perms.includes(`stage:${stage}:act:all::allow`);
  const hasStageScoped = perms.includes(`stage:${stage}:act::allow`)
    || (!!actor.dept_id && perms.includes(`stage:${stage}:act:${actor.dept_id}::allow`));
  const submitterFallback = actor.id === wb.submitter_id
    && stage === 'submission'
    && hasPermission(session.session, PERM.finance.expense.create);
  let canAct = hasPermission(session.session, PERM.admin.system.bypass)
    || hasStageAll
    || (hasStageScoped && (isCrossDeptUiStage || sameDeptAsSubmitter))
    || submitterFallback;
  if (wb.origin === 'expense' && !isRejectedStage(stage)) {
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
    canAct = (await authorizeExpenseStage(flowActor, flow)).allow;
  }
  const claimable = ['accounting_review', 'payment', 'settlement'].includes(stage);
  const claimMine = !openClaim || openClaim.claimed_by === actor.id;
  const canActNow = canAct && (!claimable || (!!openClaim && claimMine));
  const canAttach = hasPermission(session.session, 'finance:waybill:attach::allow')
    || (actor.id === wb.submitter_id && stage === 'submission' && hasPermission(session.session, PERM.finance.expense.create));
  const canSettle = stage === 'payment' && canActNow;
  const canFinalApprove = stage === 'accounting_approval' && canActNow;
  const canConfirmGl = stage === 'settlement' && canActNow;
  const canReCall = !['disbursed', 'rejected'].includes(stage)
    && (hasPermission(session.session, PERM.admin.system.bypass)
      || (wb.origin === 'expense'
        ? hasPermission(session.session, PERM.finance.expense.override)
        : hasPermission(session.session, PERM.finance.pr.override_approve)));
  const canSaveAccrual = hasPermission(session.session, 'finance:pr:edit::allow');
  const canPostAccrual = stage === 'accounting_review' && canActNow;
  const canPostSettlement = stage === 'settlement' && canActNow;
  const canReject = !['settlement', 'completed', 'rejected'].includes(stage) && canActNow;

  const isRejected = wb.status === 'rejected';
  const rejectionEvent = ctx.events.find((e) => e.kind === 'rejected') ?? null;
  const rejectionReason =
    (rejectionEvent?.payload as { reason?: string } | null)?.reason ?? null;
  const rejectionActorName = rejectionEvent?.actor_id
    ? (ctx.pipActors['rejected']?.name ?? null)
    : null;

  const originLabel =
    wb.origin === 'expense' ? <><T id="waybill.originExpense" locale={locale} /> EXP-{wb.origin_id}</>
      : wb.origin === 'pr' ? <><T id="waybill.originPr" locale={locale} /> PR-{wb.origin_id}</>
      : <><T id="waybill.originPo" locale={locale} /> PO-{wb.origin_id}</>;
  const originHref =
    wb.origin === 'expense' ? `/expense/${wb.origin_id}`
      : wb.origin === 'pr' ? `/pr/${wb.origin_id}`
      : `/po/${wb.origin_id}`;

  const pipsAll = pipsForDomain(domainForOrigin(wb.origin));
  const curIdxAll = pipIndex(domainForOrigin(wb.origin), wb.current_stage);
  const stepsDone = curIdxAll < 0 ? 0 : curIdxAll;
  const stepsTotal = pipsAll.filter((p) => p.key !== 'rejected').length;
  const progressPct =
    wb.status === 'completed' ? 100
      : wb.status === 'rejected' ? 0
      : stepsTotal > 0 ? Math.round((stepsDone / stepsTotal) * 100) : 0;

  const statusTone = wb.status === 'completed'
     ? { ring: 'from-positive/50 to-info/40', chip: 'bg-positive-soft border border-positive/40 text-positive border-positive/40', dot: 'bg-positive', label: <T id="waybill.status.completed" locale={locale} /> }
     : wb.status === 'rejected'
       ? { ring: 'from-critical/60 to-critical/20', chip: 'bg-critical-soft border border-critical/40 text-critical border-critical/40', dot: 'bg-critical', label: <T id="waybill.status.rejected" locale={locale} /> }
       : { ring: 'from-info/60 to-accent/40', chip: 'bg-info-soft border border-info/40 text-info border-info/40', dot: 'bg-info animate-pulse', label: <T id="waybill.status.inProgress" locale={locale} /> };

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
           { label: <T id="nav.home" locale={locale} />, href: '/' },
           { label: <T id="waybill.inbox.title" locale={locale} />, href: '/inbox' },
          { label: wb.id, href: `/waybill/${wb.id}` },
        ]}
      />
      <PageLayout
        width="wide"
        actions={
          <ExportPdfButton
            waybillId={wb.id}
            attachments={ctx.attachments}
            defaultSections={{ cover: true, rail: true, audit: true, attachments: true }}
          />
        }
      >
        <section className="space-y-6 font-sans">
          <WaybillHeader
            wb={wb}
            originLabel={originLabel}
            originHref={originHref}
            actor={actor}
            isRejected={isRejected}
            statusTone={statusTone}
            stepsDone={stepsDone}
            stepsTotal={stepsTotal}
            progressPct={progressPct}
            activeActorName={ctx.activeActorName}
            locale={locale}
          />

          {isRejected && (
            <section
              role="alert"
              aria-label="Rejection summary"
              data-testid="waybill-rejection-banner"
              className="panel flex flex-wrap items-start gap-3 border-critical/50 bg-critical-soft/40 px-4 py-3"
            >
              <CircleAlert className="size-5 shrink-0 text-critical mt-0.5" aria-hidden strokeWidth={2.5} />
              <div className="min-w-0 flex-1">
                <p className="text-sm font-semibold text-critical">
                  <T id="waybill.status.rejected" locale={locale} hideSecondary />
                  {rejectionActorName && (
                    <>
                      {' · '}
                      <span className="font-normal">{rejectionActorName}</span>
                    </>
                  )}
                </p>
                {rejectionReason && (
                  <p className="mt-1 text-sm text-critical/90 italic truncate" title={rejectionReason}>
                    &ldquo;{rejectionReason}&rdquo;
                  </p>
                )}
              </div>
              {actor.id === wb.submitter_id && (
                <Link
                  href={wb.origin === 'expense' ? '/expense' : wb.origin === 'pr' ? '/pr' : '/po'}
                  className="rounded-lg border border-critical/40 bg-paper-2 px-3 py-1.5 text-xs font-mono text-critical hover:bg-critical/10 transition-colors"
                >
                  <T id="waybill.resubmit" locale={locale} hideSecondary />
                </Link>
              )}
            </section>
          )}

          <div className="flex flex-wrap items-center gap-2">
            <WaybillRiskBadge waybillId={wb.id} />
            <WaybillAnomalyBadge flagged={wb.flagged_reason} />
          </div>

          <DecisionBar
            waybillId={wb.id}
            currentStage={wb.current_stage}
            status={wb.status}
            amount={wb.total_amount ?? null}
            vendorName={wb.vendor_name ?? null}
            canAct={canAct}
            canFinalApprove={canFinalApprove}
            canSettle={canSettle}
            canConfirmGl={canConfirmGl}
            isFinalApproval={wb.current_stage === 'accounting_approval'}
            isAwaitingDisbursement={wb.current_stage === 'payment'}
            isDisbursed={wb.current_stage === 'settlement'}
            isRejected={isRejected}
            actorRole={actor.role_id}
            claim={claimable ? {
              claimedBy: openClaim?.claimed_by ?? null,
              claimedByName: openClaim?.fullname ?? null,
              isMine: openClaim?.claimed_by === actor.id,
              canClaim: canAct && !openClaim,
            } : null}
          />

          <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
             <WaybillReviewHint waybillId={wb.id} lang={locale} stage="hod" label={<T id="waybill.review.hod" locale={locale} />} />
             <WaybillReviewHint waybillId={wb.id} lang={locale} stage="am" label={<T id="waybill.review.am" locale={locale} />} />
          </div>

          {action === 'reject' && canReject && !isRejected && wb.status === 'open' && (
            <InlineActionForm kind="reject" waybillId={wb.id} stage={wb.current_stage} locale={locale} />
          )}

          {action === 'final-reject' && canReject && !isRejected && wb.status === 'open' && (
            <InlineActionForm kind="final-reject" waybillId={wb.id} stage={wb.current_stage} locale={locale} />
          )}

          <WaybillTimelineBigPicture
            waybillId={wb.id}
            domain={domainForOrigin(wb.origin)}
            currentStage={wb.current_stage}
            status={wb.status}
            events={ctx.events}
            attachments={ctx.attachments}
            amountTHB={wb.total_amount ? Number(wb.total_amount) : null}
            locale={locale}
            canAct={canActNow && !isRejected}
            canAttach={canAttach && !isRejected}
            canSettle={canSettle}
            canFinalApprove={canFinalApprove}
            originId={wb.origin_id}
            approversByStage={approversByStage}
            currentUserId={actor.id}
            visionModels={visionModels}
            canConfirmGl={canConfirmGl}
            hasGlConfirmed={false}
          />

          {expensePicture && (
 <Suspense fallback={<div className="panel h-24 animate-pulse" aria-hidden />}>
              <WaybillExpenseCollapsible
                data={expensePicture}
                waybillId={wb.id}
                currentStage={wb.current_stage}
                locale={locale}
              />
            </Suspense>
          )}

          {wb.status !== 'rejected' && wb.status !== 'completed' && (
            <Suspense fallback={
            <div className="space-y-4">
              {Array.from({ length: 3 }).map((_, i) => (
 <div key={i} className="panel h-48 animate-pulse" aria-hidden />
              ))}
            </div>
          }>
            <WaybillStepCards
              wb={wb}
              waybillId={wb.id}
              currentStage={wb.current_stage}
              status={wb.status}
              events={ctx.events}
              attachments={ctx.attachments}
              approversByStage={approversByStage}
              actedUsersByStage={actedUsersByStage}
              expensePicture={null}
              hasGlConfirmed={false}
              artifacts={null}
              actorCanSeeGlLines={actorCanSeeGlLines}
              originId={wb.origin_id}
              visionModels={visionModels}
              actions={{
                canAct: canActNow,
                canAttach,
                canSettle,
                canFinalApprove,
                canConfirmGl,
                canReCall,
                canSaveAccrual,
                canPostAccrual,
                canConfirmAccrual: false,
                canPostSettlement,
                canConfirmSettlement: false,
              }}
              flags={{
                isSubmitter: actor.id === wb.submitter_id,
                isFinalApproval: wb.current_stage === 'accounting_approval',
                isDisbursed: wb.current_stage === 'settlement',
              }}
              rejection={{
                reason: rejectionReason,
                actor: rejectionEvent?.actor_id
                  ? { user_id: rejectionEvent.actor_id, fullname: rejectionActorName ?? `#${rejectionEvent.actor_id}`, role: rejectionEvent.actor_role }
                  : null,
              }}
              ui={{ action, actionStage: wb.current_stage, locale }}
            />
            </Suspense>
          )}

          <WaybillAuditSection
            waybillId={wb.id}
            events={events}
            integrity={integrity}
            locale={locale}
          />
        </section>
      </PageLayout>
    </>
  );
}
