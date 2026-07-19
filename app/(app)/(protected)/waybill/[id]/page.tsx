import React, { Suspense } from 'react';
import Link from 'next/link';
import { headers } from 'next/headers';
import { notFound, redirect } from 'next/navigation';
import { CircleAlert, ShieldCheck } from 'lucide-react';
import {
  loadWaybillRailContext,
  loadApproversByStage,
  loadActedUsersByStage,
  loadExpenseFullPicture,
} from '@/waybill/queries';
import { loadActor } from '@/server/guard';
import { query } from '@/db';
import { loadVisionModels } from '@/ai/loadVisionModels';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { getSecondaryLocale } from '@/server/locale';
import { pipsForDomain, pipIndex, domainForOrigin } from '@/waybill/derive';
import { verifyEventChain } from '@/waybill/events';
import { WaybillExpenseCollapsible } from '@/components/waybill/WaybillExpenseCollapsible';
import { WaybillHeader } from '@/components/waybill/WaybillHeader';
import { WaybillRiskBadge } from '@/components/waybill/WaybillRiskBadge';
import { WaybillAnomalyBadge } from '@/components/waybill/WaybillAnomalyBadge';
import { ExportPdfButton } from '@/components/waybill/ExportPdfButton';
import { WaybillRail } from '@/components/waybill/WaybillRail';
import { WaybillTaskPanel } from '@/components/waybill/WaybillTaskPanel';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { T } from '@/components/i18n/TServer';
import { WaybillTabs } from './_components/WaybillTabs';
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
  if (wb.origin === 'so') {
    const qs = new URLSearchParams();
    for (const [key, value] of Object.entries(sp)) {
      for (const item of Array.isArray(value) ? value : value ? [value] : []) qs.append(key, item);
    }
    redirect(`/sales/${wb.id}${qs.size ? `?${qs.toString()}` : ''}`);
  }

  const action = asString(sp.action);
  const [approversByStage, actedUsersByStage, visionModels, locale, expensePicture, integrity, openClaim] = await Promise.all([
    loadApproversByStage(wb.id),
    loadActedUsersByStage(wb.id),
    loadVisionModels(actor.id),
    getSecondaryLocale(),
    wb.origin === 'expense' ? loadExpenseFullPicture(wb.origin_id) : Promise.resolve(null),
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
    'accounting_review',
    'accounting_approval',
    'executive_approval',
    'payment',
    'settlement',
    'accounting_verification',
    'accounting_supervision',
    'accounting_authorization',
    'disbursement_authorization',
    'cfo_authorization',
    'ceo_authorization',
  ].includes(stage);
  const isDeptScopedUiStage = [
    'submission',
    'department_approval',
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
  if (wb.origin === 'expense' && wb.status !== 'rejected') {
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
  const canReCall = !['disbursed', 'settlement', 'completed', 'rejected'].includes(stage)
    && (hasPermission(session.session, PERM.admin.system.bypass)
      || (wb.origin === 'expense'
        ? hasPermission(session.session, PERM.finance.expense.override)
        : hasPermission(session.session, PERM.finance.pr.override_approve)));
  const canSaveAccrual = hasPermission(session.session, 'finance:pr:edit::allow');
  const canPostAccrual = stage === 'accounting_review' && canActNow;
  const canPostSettlement = stage === 'settlement' && canActNow;

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

  const domain = domainForOrigin(wb.origin);
  const pipsAll = pipsForDomain(domain);
  const curIdxAll = pipIndex(domain, wb.current_stage);
  const stepsDone = curIdxAll < 0 ? 0 : curIdxAll;
  const stepsTotal = pipsAll.filter((p) => p.key !== 'rejected').length;
  const progressPct =
    wb.status === 'completed' ? 100
      : wb.status === 'rejected' ? 0
      : stepsTotal > 0 ? Math.round((stepsDone / stepsTotal) * 100) : 0;

  const statusTone = wb.status === 'completed'
    ? { ring: 'from-positive/40 to-info/20', chip: 'bg-positive-soft border border-positive/40 text-positive-strong', dot: 'bg-positive', label: <T id="waybill.status.completed" locale={locale} variant="compact" /> }
    : wb.status === 'rejected'
      ? { ring: 'from-critical/50 to-critical/15', chip: 'bg-critical-soft border border-critical/40 text-critical-strong', dot: 'bg-critical', label: <T id="waybill.status.rejected" locale={locale} variant="compact" /> }
      : { ring: 'from-info/50 to-accent/30', chip: 'bg-info-soft border border-info/40 text-info-strong', dot: 'bg-info animate-pulse', label: <T id="waybill.status.inProgress" locale={locale} variant="compact" /> };

  const displayTotal = wb.total_amount ?? expensePicture?.expense.total_amount ?? null;
  const displaySubmitter = wb.submitter_name ?? expensePicture?.submitter_name ?? null;
  const displayVendor = wb.vendor_name ?? expensePicture?.expense.vendor_name ?? null;
  const hasActionSurface = !isRejected && (canActNow || (claimable && canAct && !openClaim));

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
          { label: <T id="nav.home" locale={locale} />, href: '/' },
          { label: <T id="waybill.inbox.title" locale={locale} />, href: '/inbox' },
          { label: wb.id, href: `/waybill/${wb.id}` },
        ]}
      />
      <PageLayout width="wide">
        <section className="space-y-5 font-sans">
          <WaybillHeader
            wb={wb}
            originLabel={originLabel}
            originHref={originHref}
            actor={actor}
            submitterName={displaySubmitter}
            vendorName={displayVendor}
            totalAmount={displayTotal}
            isRejected={isRejected}
            statusTone={statusTone}
            stepsDone={stepsDone}
            stepsTotal={stepsTotal}
            progressPct={progressPct}
            locale={locale}
            actions={
              <ExportPdfButton
                waybillId={wb.id}
                attachments={ctx.attachments}
                defaultSections={{ cover: true, rail: true, audit: true, attachments: true }}
              />
            }
          />

          <WaybillTabs waybillId={wb.id} active="overview" locale={locale} />

          {isRejected && (
            <section
              role="alert"
              aria-label="Rejection summary"
              data-testid="waybill-rejection-banner"
              className="panel flex flex-wrap items-start gap-3 border-critical/50 bg-critical-soft/50 px-4 py-3"
            >
              <CircleAlert className="mt-0.5 size-5 shrink-0 text-critical" aria-hidden strokeWidth={2.5} />
              <div className="min-w-0 flex-1">
                <p className="text-sm font-semibold text-critical">
                  <T id="waybill.status.rejected" locale={locale} hideSecondary />
                  {rejectionActorName && <> · <span className="font-normal">{rejectionActorName}</span></>}
                </p>
                {rejectionReason && (
                  <p className="mt-1 truncate text-sm italic text-critical/90" title={rejectionReason}>
                    &ldquo;{rejectionReason}&rdquo;
                  </p>
                )}
              </div>
              {actor.id === wb.submitter_id && (
                <Link
                  href={wb.origin === 'expense' ? '/expense' : wb.origin === 'pr' ? '/pr' : '/po'}
                  className="rounded-lg border border-critical/40 bg-paper-2 px-3 py-2 text-xs font-semibold text-critical transition hover:bg-critical/10"
                >
                  <T id="waybill.resubmit" locale={locale} hideSecondary />
                </Link>
              )}
            </section>
          )}

          <WaybillRail
            wb={wb}
            currentStage={wb.current_stage}
            status={wb.status}
            canAct={canActNow}
            indicators={
              <>
                <WaybillRiskBadge waybillId={wb.id} />
                <WaybillAnomalyBadge flagged={wb.flagged_reason} />
              </>
            }
          />

          <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(22rem,27rem)] lg:items-start">
            <main className="min-w-0 space-y-5">
              {expensePicture ? (
                <Suspense fallback={<div className="panel h-56 animate-pulse" aria-hidden />}>
                  <WaybillExpenseCollapsible
                    data={expensePicture}
                    waybillId={wb.id}
                    currentStage={wb.current_stage}
                    locale={locale}
                  />
                </Suspense>
              ) : (
                <section className="panel flex items-center gap-3 p-4">
                  <ShieldCheck className="size-5 text-info" aria-hidden />
                  <div>
                    <h2 className="text-sm font-semibold text-ink"><T id="waybill.overview.evidence" locale={locale} variant="stacked" /></h2>
                    <p className="mt-1 text-sm text-ink-2"><T id="waybill.overview.evidenceHint" locale={locale} variant="stacked" /></p>
                  </div>
                </section>
              )}

              <section className="panel flex flex-wrap items-center justify-between gap-3 p-4">
                <div className="flex min-w-0 items-center gap-3">
                  <ShieldCheck className={integrity.ok ? 'size-5 text-positive' : 'size-5 text-critical'} aria-hidden />
                  <div className="min-w-0">
                    <h2 className="text-sm font-semibold text-ink"><T id="waybill.audit.logTitle" locale={locale} variant="stacked" /></h2>
                    <p className="mt-1 truncate text-xs text-ink-2">{integrity.total} · <T id="waybill.audit.appendOnly" locale={locale} variant="compact" /></p>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <span className={integrity.ok ? 'text-xs font-semibold text-positive' : 'text-xs font-semibold text-critical'}>
                    <T id={integrity.ok ? 'waybill.audit.hmacVerified' : 'waybill.audit.integrityFailed'} locale={locale} variant="compact" />
                  </span>
                  <Link href={`/waybill/${wb.id}/audit`} className="rounded-lg border border-rule px-3 py-2 text-xs font-semibold text-info transition hover:border-info">
                    <T id="waybill.overview.openAudit" locale={locale} variant="compact" />
                  </Link>
                </div>
              </section>

            </main>

            <aside className="min-w-0 lg:sticky lg:top-[5.5rem]">
              <WaybillTaskPanel
                wb={wb}
                waybillId={wb.id}
                currentStage={wb.current_stage}
                status={wb.status}
                events={ctx.events}
                attachments={ctx.attachments}
                approversByStage={approversByStage}
                actedUsersByStage={actedUsersByStage}
                actorCanSeeGlLines={actorCanSeeGlLines}
                originId={wb.origin_id}
                visionModels={visionModels}
                actions={{
                  canAct: canActNow && !isRejected,
                  canAttach: canAttach && !isRejected,
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
                ui={{ action, actionStage: asString(sp.stage) ?? wb.current_stage, locale }}
                claim={claimable ? {
                  claimedBy: openClaim?.claimed_by ?? null,
                  claimedByName: openClaim?.fullname ?? null,
                  isMine: openClaim?.claimed_by === actor.id,
                  canClaim: canAct && !openClaim,
                } : null}
                currentUserId={actor.id}
                action={action}
                domain={domain}
                locale={locale}
                vendorName={displayVendor}
                amount={displayTotal}
              />
            </aside>
          </div>

          {hasActionSurface && (
            <div className="fixed inset-x-0 bottom-0 z-fixed border-t border-rule bg-[color-mix(in_oklab,var(--glass-floating)_94%,transparent)] p-3 shadow-modal backdrop-blur-xl lg:hidden safe-bottom">
              <a href="#waybill-task" className="flex min-h-11 items-center justify-center rounded-lg bg-info px-4 py-2.5 text-sm font-semibold text-paper transition hover:bg-info-strong">
                <T id="waybill.overview.reviewTask" locale={locale} variant="compact" />
              </a>
            </div>
          )}
        </section>
      </PageLayout>
    </>
  );
}
