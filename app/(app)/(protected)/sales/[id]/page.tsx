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
} from '@/waybill/queries';
import { loadActor } from '@/server/guard';
import { loadVisionModels } from '@/ai/loadVisionModels';
import { hasPermission, loadActivePermSession, type PermSession } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { STAGE_TO_PERM, stageRoles } from '@/perm';
import { getSecondaryLocale } from '@/server/locale';
import { pipsForDomain, pipIndex, domainForOrigin } from '@/waybill/derive';
import { verifyEventChain } from '@/waybill/events';
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
import { SalesExtractPanel, type SoItemRow } from '@/components/waybill/SalesExtractPanel';
import { query } from '@/db';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ id: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}

function asString(v: string | string[] | undefined): string | null {
  if (Array.isArray(v)) return v[0] ?? null;
  return v ?? null;
}

function canActOnSalesStage(session: PermSession, roleName: string, stage: string): boolean {
  if (hasPermission(session, PERM.admin.system.bypass)) return true;
  const stagePerm = STAGE_TO_PERM[stage];
  if (stagePerm && hasPermission(session, stagePerm)) return true;
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

  const [approversByStage, actedUsersByStage, visionModels, locale, salesJournal, events, integrity, soItemsRes, arSlipRes] = await Promise.all([
    loadApproversByStage(wb.id),
    loadActedUsersByStage(wb.id),
    loadVisionModels(),
    getSecondaryLocale(),
    loadJournalForWaybill(wb.id),
    loadWaybillEvents(wb.id),
    verifyEventChain(wb.id),
    query<SoItemRow>(
      `SELECT id, description, qty::float AS qty, unit_price::float AS unit_price,
              vat_amount::float AS vat_amount, line_total::float AS line_total,
              mapped_revenue_account_code, confidence_score::float AS confidence_score
         FROM so_items
        WHERE sales_order_id = $1
        ORDER BY id ASC`,
      [wb.origin_id],
    ),
    query<{ ar_slip_id: number | null }>(
      `SELECT ar_slip_id FROM sales_orders WHERE id = $1`,
      [wb.origin_id],
    ),
  ]);

  const soItems = soItemsRes.rows.map((r) => ({
    ...r,
    qty: Number(r.qty),
    unit_price: Number(r.unit_price),
    vat_amount: Number(r.vat_amount),
    line_total: Number(r.line_total),
    confidence_score: r.confidence_score == null ? null : Number(r.confidence_score),
  }));
  const existingArSlipId = arSlipRes.rows[0]?.ar_slip_id ?? null;

  const actorRole = actor.role_name;
  const session = permOut.session;

  const actorCanSeeGlLines = hasPermission(session, 'finance:gl:view::allow');
  const canPostSalesGlVat = hasPermission(session, 'finance:gl:post::allow');
  const canPostSalesGlAccrual = hasPermission(session, 'finance:gl:post::allow');
  const canPostSalesGlSettlement = hasPermission(session, 'finance:gl:post::allow');
  const canConfirmSalesGl = hasPermission(session, 'finance:gl:confirm::allow');
  const canSettle = hasPermission(session, 'finance:sales:settle::allow');

  const canRecordSalesPayment =
    hasPermission(session, PERM.admin.system.bypass) ||
    hasPermission(session, 'finance:sales:settle::allow') ||
    ['finance', 'account_officer', 'account_supervisor', 'accounting_manager', 'cfo', 'ceo'].includes(actorRole);

  const rejectionEvent = ctx.events.find((e) => e.kind === 'so-rejected') ?? null;
  const rejectionReason = (rejectionEvent?.payload as { reason?: string } | null)?.reason ?? null;

  const pipsAll = pipsForDomain(domainForOrigin(wb.origin));
  const curIdxAll = pipIndex(domainForOrigin(wb.origin), wb.current_stage);
  const stepsDone = curIdxAll < 0 ? 0 : curIdxAll;
  const stepsTotal = pipsAll.filter((p) => p.key !== 'rejected').length;
  const progressPct = wb.status === 'completed' ? 100
    : stepsTotal > 0 ? Math.round((stepsDone / stepsTotal) * 100) : 0;
  const statusTone = wb.status === 'completed' ? { ring: 'from-positive/50 to-info/40', chip: 'bg-positive text-positive border-positive/40', dot: 'bg-positive', label: 'Completed' }
    : wb.status === 'rejected' ? { ring: 'from-critical/60 to-critical/20', chip: 'bg-critical text-critical border-critical/40', dot: 'bg-critical', label: 'Rejected' }
      : { ring: 'from-info/60 to-accent/40', chip: 'bg-info text-info border-info/40', dot: 'bg-info animate-pulse', label: 'In progress' };

  const canAct = canActOnSalesStage(session, actorRole, wb.current_stage);

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
              <div key={i} className="h-48 animate-pulse rounded-md border border-rule/60 bg-paper-2/50" aria-hidden />
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

          <section className="space-y-3">
            <header className="flex items-baseline justify-between">
              <h2 className="text-sm font-mono uppercase tracking-widest text-ink-2">
                Record to GL
              </h2>
              <span className="text-xs font-mono text-mute">
                stage · {wb.current_stage}
              </span>
            </header>
            <SalesExtractPanel
              lang={locale as 'en' | 'th' | 'de'}
              onUse={() => {}}
              waybillId={wb.id}
              soId={wb.origin_id}
              soItems={soItems}
              existingArSlipId={existingArSlipId}
              canRecord={canRecordSalesPayment}
            />
          </section>

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
