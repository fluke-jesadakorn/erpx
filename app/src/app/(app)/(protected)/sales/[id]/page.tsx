import React from 'react';
import { Suspense } from 'react';
import { headers } from 'next/headers';
import { redirect, notFound } from 'next/navigation';
import {
  loadWaybillRailContext,
  loadApproversByStage,
  loadActedUsersByStage,
  loadJournalForWaybill,
  loadSalesArtifacts,
  loadWaybillEvents,
  type WaybillJournalView,
} from '@folio-lib/waybill/queries';
import { loadActor } from '@folio-lib/server/guard';
import { loadVisionModels } from '@folio-lib/ai/loadVisionModels';
import { loadActivePermSession, hasPermission, matchPerm, PERM } from '@folio-lib/perm/server';
import { STAGE_TO_PERM, stageRoles } from '@folio-lib/perm';
import { getSecondaryLocale } from '@folio-lib/server/locale';
import { pipsForDomain, pipIndex, domainForOrigin } from '@folio-lib/waybill/derive';
import { verifyEventChain } from '@folio-lib/waybill/events';
import { WaybillStepCards } from '@/components/waybill/WaybillStepCards';
import { WaybillGlSection } from '@/components/waybill/WaybillGlSection';
import { WaybillTimelineBigPicture } from '@/components/waybill/WaybillTimelineBigPicture';
import { WaybillAuditSection } from '@/components/waybill/WaybillAuditSection';
import { WaybillHeader } from '@/components/waybill/WaybillHeader';
import { InlineActionForm } from '@/components/waybill/InlineActionForm';
import { ExportPdfButton } from '@/components/waybill/ExportPdfButton';
import { DecisionBar } from '@/components/waybill/DecisionBar';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { SalesPipPanel } from './_components/SalesPipPanel';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ id: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}

function asString(v: string | string[] | undefined): string | null {
  if (Array.isArray(v)) return v[0] ?? null;
  return v ?? null;
}

function canActOnSalesStage(perms: string[], roleName: string, stage: string): boolean {
  if (matchPerm(perms, 'admin:system:bypass::allow')) return true;
  const stagePerm = STAGE_TO_PERM[stage];
  if (stagePerm && matchPerm(perms, stagePerm)) return true;
  const roles = stageRoles(stage);
  return roles.includes(roleName);
}

export default async function SalesDetailPage({ params, searchParams }: PageProps) {
  const { id } = await params;
  const sp = await searchParams;

  const h = await headers();
  const permOut = await loadActivePermSession(
    new Request('http://internal', { headers: h as unknown as HeadersInit }),
  );
  if (!permOut || !hasPermission(permOut.session, PERM.tile.sales.view)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Hub', href: '/' }, { label: 'Sales', href: '/sales' }, { label: id, href: `/sales/${id}` }]} />
        <PageLayout title={id} subtitle={permOut?.session.user.name ?? undefined}>
          <NoPermissionView
            kind="locked"
            actor={permOut ? (permOut.session.user as any) : null}
            attemptedPath={`/sales/${id}`}
            reason={permOut ? 'tile:sales:view required.' : 'Sign in to view this page.'}
          />
        </PageLayout>
      </>
    );
  }

  const actor = await loadActor();
  if (!actor) redirect('/login');

  const ctx = await loadWaybillRailContext(id);
  if (!ctx || ctx.waybill.origin !== 'so') notFound();

  const wb = ctx.waybill;
  const action = asString(sp.action);

  const salesArtifacts = await loadSalesArtifacts(wb);

  const [approversByStage, actedUsersByStage, visionModels, locale, salesJournal, events, integrity] = await Promise.all([
    loadApproversByStage(wb.id),
    loadActedUsersByStage(wb.id),
    loadVisionModels(),
    getSecondaryLocale(),
    loadJournalForWaybill(wb.id),
    loadWaybillEvents(wb.id),
    verifyEventChain(wb.id),
  ]);

  const perms = actor.permissions;
  const actorRole = actor.role_name;

  const actorCanSeeGlLines = matchPerm(perms, 'finance:gl:view::allow');
  const canPostSalesGlVat = matchPerm(perms, 'finance:gl:post::allow');
  const canPostSalesGlAccrual = matchPerm(perms, 'finance:gl:post::allow');
  const canPostSalesGlSettlement = matchPerm(perms, 'finance:gl:post::allow');
  const canConfirmSalesGl = matchPerm(perms, 'finance:gl:confirm::allow');
  const canSettle = matchPerm(perms, 'finance:sales:settle::allow');

  const rejectionEvent = ctx.events.find((e) => e.kind === 'so-rejected') ?? null;
  const rejectionReason = (rejectionEvent?.payload as { reason?: string } | null)?.reason ?? null;

  const pipsAll = pipsForDomain(domainForOrigin(wb.origin));
  const curIdxAll = pipIndex(domainForOrigin(wb.origin), wb.current_stage);
  const stepsDone = curIdxAll < 0 ? 0 : curIdxAll;
  const stepsTotal = pipsAll.filter((p) => p.key !== 'rejected').length;
  const progressPct = wb.status === 'completed' ? 100
    : stepsTotal > 0 ? Math.round((stepsDone / stepsTotal) * 100) : 0;
  const statusTone = wb.status === 'completed' ? { ring: 'from-emerald-500/50 to-cyan-500/40', chip: 'bg-emerald-500/15 text-emerald-200 border-emerald-400/50', dot: 'bg-emerald-400', label: 'Completed' }
    : wb.status === 'rejected' ? { ring: 'from-rose-500/60 to-rose-500/20', chip: 'bg-rose-500/15 text-rose-200 border-rose-400/50', dot: 'bg-rose-400', label: 'Rejected' }
      : { ring: 'from-cyan-500/60 to-indigo-500/40', chip: 'bg-cyan-500/15 text-cyan-200 border-cyan-400/50', dot: 'bg-cyan-400 animate-pulse', label: 'In progress' };

  const canAct = canActOnSalesStage(perms, actorRole, wb.current_stage);

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
          { label: 'Hub', href: '/' },
          { label: 'Sales', href: '/sales' },
          { label: wb.id, href: `/waybill/${wb.id}` },
        ]}
      />
      <PageLayout
        title={wb.id}
        subtitle={`SO · ${wb.vendor_name ?? '—'}`}
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
            originLabel={`SO-${wb.origin_id}`}
            originHref="/sales"
            actor={actor}
            isRejected={wb.status === 'rejected'}
            statusTone={statusTone}
            stepsDone={stepsDone}
            stepsTotal={stepsTotal}
            progressPct={progressPct}
            activeActorName={ctx.activeActorName}
          />

          <DecisionBar
            waybillId={wb.id}
            currentStage={wb.current_stage}
            status={wb.status}
            amount={wb.total_amount ?? null}
            vendorName={wb.vendor_name ?? null}
            canAct={canAct}
            canFinalApprove={false}
            canSettle={canSettle}
            canConfirmGl={canConfirmSalesGl}
            isFinalApproval={wb.current_stage === 'so_invoiced'}
            isAwaitingDisbursement={wb.current_stage === 'so_invoiced'}
            isDisbursed={wb.current_stage === 'so_paid'}
            isRejected={wb.status === 'rejected'}
            actorRole={actor.role_name ?? null}
          />

          {action === 'reject' && canAct && !rejectionReason && wb.status === 'open' && (
            <InlineActionForm kind="reject" waybillId={wb.id} stage={wb.current_stage} locale={locale} />
          )}

          <WaybillTimelineBigPicture
            waybillId={wb.id}
            domain="sales"
            currentStage={wb.current_stage}
            status={wb.status as 'open' | 'completed' | 'rejected' | 'reversed' | 'superseded'}
            events={ctx.events}
            attachments={ctx.attachments}
            amountTHB={wb.total_amount ? Number(wb.total_amount) : null}
            locale={locale}
            canAct={canAct && wb.status !== 'rejected'}
            canAttach={false}
            canSettle={canSettle}
            canFinalApprove={false}
            originId={wb.origin_id}
            approversByStage={approversByStage}
            currentUserId={actor.id}
            visionModels={visionModels}
            canConfirmGl={canConfirmSalesGl}
            hasGlConfirmed={false}
          />

          <Suspense fallback={<div className="space-y-4">
            {Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="h-48 animate-pulse rounded-2xl border border-slate-800/60 bg-slate-950/40" aria-hidden />
            ))}
          </div>}>
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
              artifacts={salesArtifacts as never}
              actorCanSeeGlLines={canConfirmSalesGl}
              originId={wb.origin_id}
              visionModels={visionModels}
              actions={{
                canAct,
                canAttach: false,
                canSettle,
                canFinalApprove: false,
                canConfirmGl: canConfirmSalesGl,
                canReCall: false,
                canSaveAccrual: false,
                canPostAccrual: canPostSalesGlVat,
                canConfirmAccrual: false,
                canPostSettlement: canPostSalesGlSettlement,
                canConfirmSettlement: canConfirmSalesGl,
              }}
              flags={{
                isSubmitter: actor.id === wb.submitter_id,
                isFinalApproval: wb.current_stage === 'so_invoiced',
                isDisbursed: wb.current_stage === 'so_paid',
              }}
              rejection={{ reason: rejectionReason, actor: null }}
              ui={{ action, actionStage: null, locale }}
            />
          </Suspense>

          {salesArtifacts ? (
            <SalesPipPanel
              waybillId={wb.id}
              artifacts={salesArtifacts}
              currentStage={wb.current_stage}
            />
          ) : null}

          <WaybillGlSection
            waybillId={wb.id}
            origin="so"
            journal={(salesJournal as unknown as WaybillJournalView)}
            actorRole={actor.role_name ?? null}
            actorCanSeeLines={actorCanSeeGlLines}
            lang={locale}
            canPostSalesGlVat={canPostSalesGlVat}
            canPostSalesGlAccrual={canPostSalesGlAccrual}
            canPostSalesGlSettlement={canPostSalesGlSettlement}
            canConfirmSalesGl={canConfirmSalesGl}
          />

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
