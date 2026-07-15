import React, { Suspense } from 'react';
import { notFound, redirect } from 'next/navigation';
import {
  loadWaybillRailContext,
  loadApproversByStage,
  loadActedUsersByStage,
  loadExpenseFullPicture,
  loadWaybillEvents,
} from '@folio-lib/waybill/queries';
import { loadActor } from '@folio-lib/server/guard';
import { loadVisionModels } from '@folio-lib/ai/loadVisionModels';
import { matchPerm } from '@folio-lib/perm/server';
import { getSecondaryLocale } from '@folio-lib/server/locale';
import { pipsForDomain, pipIndex, domainForOrigin } from '@folio-lib/waybill/derive';
import { verifyEventChain } from '@folio-lib/waybill/events';
import { WaybillStepCards } from '@/components/waybill/WaybillStepCards';
import { WaybillTimelineBigPicture } from '@/components/waybill/WaybillTimelineBigPicture';
import { WaybillAuditSection } from '@/components/waybill/WaybillAuditSection';
import { WaybillExpenseCollapsible } from '@/components/waybill/WaybillExpenseCollapsible';
import { WaybillHeader } from '@/components/waybill/WaybillHeader';
import { InlineActionForm } from '@/components/waybill/InlineActionForm';
import { ExportPdfButton } from '@/components/waybill/ExportPdfButton';
import { DecisionBar } from '@/components/waybill/DecisionBar';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ id: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}

function asString(v: string | string[] | undefined): string | null {
  if (Array.isArray(v)) return v[0] ?? null;
  return v ?? null;
}

function isRecallRole(roleName: string): boolean {
  return roleName === 'cfo' || roleName === 'ceo' || roleName === 'finance' || roleName === 'admin';
}

function isFinalApproveStage(stage: string): boolean {
  return stage === 'accounting_authorization';
}

export default async function WaybillDetail({ params, searchParams }: PageProps) {
  const { id } = await params;
  const sp = await searchParams;

  const actor = await loadActor();
  if (!actor) redirect('/login');

  const ctx = await loadWaybillRailContext(id);
  if (!ctx) notFound();

  const wb = ctx.waybill;
  if (wb.origin === 'so') redirect(`/sales/${wb.origin_id}`);

  const action = asString(sp.action);

  const [approversByStage, actedUsersByStage, visionModels, locale, expensePicture, events, integrity] = await Promise.all([
    loadApproversByStage(wb.id),
    loadActedUsersByStage(wb.id),
    loadVisionModels(),
    getSecondaryLocale(),
    wb.origin === 'expense' ? loadExpenseFullPicture(wb.origin_id) : Promise.resolve(null),
    loadWaybillEvents(wb.id),
    verifyEventChain(wb.id),
  ]);

  const perms = actor.permissions;
  const stage = wb.current_stage;
  const actorCanSeeGlLines = matchPerm(perms, 'finance:gl:view::allow');
  const canAct = matchPerm(perms, `stage:${stage}:act::allow`)
    || matchPerm(perms, `stage:${stage}:act:all::allow`)
    || matchPerm(perms, 'admin:system:bypass::allow');
  const canAttach = matchPerm(perms, 'finance:waybill:attach::allow')
    || (actor.id === wb.submitter_id && stage === 'submission' && matchPerm(perms, 'finance:expense:create::allow'));
  const canSettle = stage === 'awaiting_disbursement'
    && matchPerm(perms, 'finance:expense:settle::allow');
  const canFinalApprove = isFinalApproveStage(stage)
    && matchPerm(perms, 'finance:expense:approve::allow');
  const canConfirmGl = stage === 'disbursed'
    && matchPerm(perms, 'finance:gl:confirm::allow');
  const canReCall = !['disbursed', 'rejected'].includes(stage)
    && isRecallRole(actor.role_name);
  const canSaveAccrual = matchPerm(perms, 'finance:pr:edit::allow');
  const canPostAccrual = stage === 'accounting_authorization'
    && matchPerm(perms, 'finance:gl:post::allow');
  const canPostSettlement = stage === 'disbursed'
    && matchPerm(perms, 'finance:gl:post::allow');
  const canReject = !['disbursed', 'gl_confirmed', 'rejected'].includes(stage)
    && (matchPerm(perms, 'admin:system:bypass::allow')
      || actor.role_name === 'cfo'
      || actor.role_name === 'ceo'
      || actor.role_name === 'admin'
      || actor.role_name === 'finance'
      || actor.role_name === 'account_officer'
      || actor.role_name === 'account_supervisor'
      || actor.role_name === 'accounting_manager');

  const isRejected = wb.status === 'rejected';
  const rejectionEvent = ctx.events.find((e) => e.kind === 'rejected') ?? null;
  const rejectionReason =
    (rejectionEvent?.payload as { reason?: string } | null)?.reason ?? null;
  const rejectionActorName = rejectionEvent?.actor_id
    ? (ctx.pipActors['rejected']?.name ?? null)
    : null;

  const originLabel =
    wb.origin === 'expense' ? `Expense EXP-${wb.origin_id}`
      : wb.origin === 'pr' ? `PR-${wb.origin_id}`
      : `PO-${wb.origin_id}`;
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
      : stepsTotal > 0 ? Math.round((stepsDone / stepsTotal) * 100) : 0;

  const statusTone = wb.status === 'completed'
    ? { ring: 'from-emerald-500/50 to-cyan-500/40', chip: 'bg-emerald-500/15 text-emerald-200 border-emerald-400/50', dot: 'bg-emerald-400', label: 'Completed' }
    : wb.status === 'rejected'
      ? { ring: 'from-rose-500/60 to-rose-500/20', chip: 'bg-rose-500/15 text-rose-200 border-rose-400/50', dot: 'bg-rose-400', label: 'Rejected' }
      : { ring: 'from-cyan-500/60 to-indigo-500/40', chip: 'bg-cyan-500/15 text-cyan-200 border-cyan-400/50', dot: 'bg-cyan-400 animate-pulse', label: 'In progress' };

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
          { label: 'Hub', href: '/' },
          { label: 'Waybills', href: '/my-waybills' },
          { label: wb.id, href: `/waybill/${wb.id}` },
        ]}
      />
      <PageLayout
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
            isFinalApproval={wb.current_stage === 'accounting_authorization'}
            isAwaitingDisbursement={wb.current_stage === 'awaiting_disbursement'}
            isDisbursed={wb.current_stage === 'disbursed'}
            isRejected={isRejected}
            actorRole={actor.role_id}
          />

          {action === 'reject' && canReject && !isRejected && wb.status === 'open' && (
            <InlineActionForm kind="reject" waybillId={wb.id} stage={wb.current_stage} locale={locale} />
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
            canAct={canAct && !isRejected}
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
            <Suspense fallback={<div className="h-24 animate-pulse rounded-2xl border border-slate-800/60 bg-slate-950/40" aria-hidden />}>
              <WaybillExpenseCollapsible
                data={expensePicture}
                waybillId={wb.id}
                currentStage={wb.current_stage}
                locale={locale}
              />
            </Suspense>
          )}

          <Suspense fallback={
            <div className="space-y-4">
              {Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="h-48 animate-pulse rounded-2xl border border-slate-800/60 bg-slate-950/40" aria-hidden />
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
                canAct,
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
                isFinalApproval: wb.current_stage === 'accounting_authorization',
                isDisbursed: wb.current_stage === 'disbursed',
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
