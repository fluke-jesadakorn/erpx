import React from 'react';
import Link from 'next/link';
import type { WaybillDomain, WaybillStagePip, PipState } from '@/waybill/derive';
import type { SecondaryLocale } from '@/server/locale';
import {
  pipsForDomain,
  pipIndex,
  bucketLabel,
  computePipState,
  stageRoleLabel,
  lastAdvancedEvent,
} from '@/waybill/derive';
import type { WaybillEventRow } from '@/waybill/events';
import type { WaybillAttachmentRow } from '@/waybill/attachments';
import {
  WAYBILL_KINDS,
  allowedKindsFor,
  type WaybillAttachmentKind,
} from '@/waybill/kinds';
import { AttachmentRow } from './AttachmentRow';
import { AttachmentUpload } from './AttachmentUpload';
import type { ApproversByStage } from '@/waybill/queries';
import { approveWaybillAction, confirmGlRecordedAction, finalApproveWaybillAction } from '@/app/actions/waybill';
import { SettleForm } from '@/app/(app)/(protected)/waybill/[id]/_components/SettleForm';
import type { VisionModel } from '@/ai/loadVisionModels';
import { roleDisplay } from './ui';
import { formatDateServer } from '@/components/i18n/formattersServer';
import { ApproversList } from './ApproversList';
import { T } from '@/components/i18n/T';

export interface WaybillTimelineBigPictureProps {
  waybillId: string;
  domain: WaybillDomain;
  currentStage: string;
  status: 'open' | 'completed' | 'rejected' | 'reversed' | 'superseded';
  events: WaybillEventRow[];
  attachments: WaybillAttachmentRow[];
  amountTHB?: number | null;
  locale?: SecondaryLocale;
  canAct: boolean;
  canAttach: boolean;
  canSettle: boolean;
  canFinalApprove: boolean;
  originId: number | null;
  approversByStage?: ApproversByStage;
  currentUserId?: number | null;
  visionModels?: VisionModel[];
  canConfirmGl?: boolean;
  hasGlConfirmed?: boolean;
}

const PIP_BADGE_EN: Record<PipState, string> = {
  passed: 'waybill.pip.done',
  active: 'waybill.pip.yourTurn',
  pending: 'waybill.pip.pending',
  rejected: 'waybill.pip.stop',
  skipped: 'waybill.pip.skip',
};

function toneForState(state: PipState) {
  switch (state) {
    case 'passed':
      return {
        card: 'border-positive bg-positive-soft border border-positive/40 text-positive',
        bullet: 'bg-positive shadow-[0_0_12px_rgba(52,211,153,0.7)] text-positive',
        badge: 'bg-positive-soft border border-positive/40 text-positive border bg-positive-soft border border-positive/40',
        title: 'text-positive',
        sectionHead: 'text-positive/80',
      };
    case 'active':
      return {
        card: 'border-info bg-info-strong text-ink ring-2 ring-info',
        bullet: 'bg-info shadow-[0_0_14px_rgba(34,211,238,0.9)] animate-pulse text-info',
        badge: 'bg-info text-ink border border-info',
        title: 'text-info',
        sectionHead: 'text-info/80',
      };
    case 'skipped':
      return {
 card: 'border-rule bg-paper-2 border border-rule text-[var(--text-ghost)] opacity-60',
 bullet: 'bg-paper-2 border border-rule text-[var(--text-faint)]',
 badge: 'bg-paper-2 border border-rule text-[var(--text-faint)] border ',
        title: 'text-[var(--text-ghost)]',
        sectionHead: 'text-[var(--text-ghost)]',
      };
    case 'rejected':
      return {
        card: 'border-critical bg-critical-soft border border-critical/40 text-critical ring-2 ring-critical',
        bullet: 'bg-critical text-critical shadow-[0_0_10px_rgba(244,63,94,0.7)]',
        badge: 'bg-critical text-critical border border-critical',
        title: 'text-critical',
        sectionHead: 'text-critical/80',
      };
    case 'pending':
    default:
      return {
 card: ' bg-paper-2 border border-rule text-mute',
        bullet: 'bg-paper-3 text-[var(--text-faint)]',
 badge: 'bg-paper-2 border border-rule text-[var(--text-faint)] border ',
        title: 'text-mute',
        sectionHead: 'text-[var(--text-faint)]',
      };
  }
}

export function WaybillTimelineBigPicture({
  waybillId,
  domain,
  currentStage,
  status,
  events,
  attachments,
  amountTHB,
  locale,
  canAct,
  canAttach,
  canSettle,
  canFinalApprove,
  originId,
  approversByStage = {},
  currentUserId = null,
  visionModels = [],
  canConfirmGl = false,
  hasGlConfirmed = false,
}: WaybillTimelineBigPictureProps) {
  const localeSafe: SecondaryLocale = locale ?? 'th';
  void localeSafe;
  const pips = pipsForDomain(domain);
  const curIdx = pipIndex(domain, currentStage);
  const isRejected = status === 'rejected' || currentStage === 'rejected';

  void amountTHB;

  const items = pips
    .map((pip, idx) => ({ pip, idx, state: computePipState(pip, idx, curIdx, currentStage, status) }))
    .slice()
    .reverse();

  const totalActive = items.filter((i) => i.state === 'active').length;
  void totalActive;

  return (
    <section
      aria-label={localeSafe === 'de' ? 'Waybill-Pipeline (vollständig)' : 'ไปป์ไลน์ Waybill (แบบเต็ม)'}
      className="space-y-4 font-sans"
    >
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <span aria-hidden className="grid h-9 w-9 place-items-center rounded-md bg-info-soft text-lg ring-1 ring-info">
            🚦
          </span>
          <div className="flex flex-col">
            <span className="text-base font-bold text-ink sm:text-lg">
              <T id={domain === 'procurement' ? 'waybill.timeline.procurementPipeline' : 'waybill.timeline.expensePipeline'} />
            </span>
            <span className="text-xs uppercase tracking-widest text-[var(--text-faint)]">
              {<T id="waybill.timeline.bottom_top_approval_order" />}
            </span>
          </div>
        </div>
 <span className="rounded-full border bg-paper-2 border border-rule px-3 py-1 text-xs font-mono text-mute">
           {isRejected ? (
             <T id="waybill.timeline.closed_rejected" />
           ) : (
             <>
               <T id="waybill.timeline.step" /> {curIdx < 0 ? items.length : curIdx + 1}{' '}
               <T id="waybill.timeline.of" /> {items.length}
             </>
           )}
        </span>
      </header>

      <div className="flex items-stretch gap-5">
        <div className="relative flex w-8 shrink-0 flex-col items-center pt-2 pb-2">
          <span aria-hidden className="z-10 mb-1 text-lg leading-none text-info drop-shadow-[0_0_8px_rgba(34,211,238,0.7)]">▲</span>
          <span className="z-10 h-1 w-4 rounded bg-info" />
          <div className="relative z-0 h-full w-1 flex-1 rounded-full bg-info" />
        </div>

        <div className="flex min-w-0 flex-1 flex-col gap-4">
          {items.map(({ pip, idx, state }) => {
            const roleText = stageRoleLabel(pip.key, locale);
            const pipEvents = events
              .filter((e) => e.stage_from === pip.key || e.stage_to === pip.key)
              .sort((a, b) => a.sequence - b.sequence);
            const pipAttachments = attachments.filter((a) => a.stage_key === pip.key);
            const allowedKinds = allowedKindsFor(pip.key);
            const isCurrentStage = currentStage === pip.key;
            const bucketKind = bucketLabel(pip.bucket, locale);
            return (
              <StepCard
                key={pip.key}
                waybillId={waybillId}
                pip={pip}
                pipIndexN={idx}
                state={state}
                roleText={roleText}
                bucketKind={bucketKind}
                events={pipEvents}
                attachments={pipAttachments}
                allowedKinds={allowedKinds}
                isCurrentStage={isCurrentStage}
                canAct={isCurrentStage && canAct && !isRejected}
                canAttach={isCurrentStage && canAttach && !isRejected}
                canSettle={isCurrentStage && canSettle && !isRejected}
                canFinalApprove={isCurrentStage && canFinalApprove && !isRejected}
                originId={originId}
                locale={localeSafe}
                approvers={approversByStage[pip.key] ?? []}
                currentUserId={currentUserId}
                visionModels={visionModels}
                canConfirmGl={isCurrentStage && pip.key === 'disbursed' && canConfirmGl}
                glConfirmed={hasGlConfirmed}
              />
            );
          })}
        </div>
      </div>
    </section>
  );
}

interface StepCardProps {
  waybillId: string;
  pip: WaybillStagePip;
  pipIndexN: number;
  state: PipState;
  roleText: string;
  bucketKind: string;
  events: WaybillEventRow[];
  attachments: WaybillAttachmentRow[];
  allowedKinds: WaybillAttachmentKind[];
  isCurrentStage: boolean;
  canAct: boolean;
  canAttach: boolean;
  canSettle: boolean;
  canFinalApprove: boolean;
  originId: number | null;
  locale: SecondaryLocale;
  approvers: ApproversByStage[string];
  currentUserId: number | null;
  visionModels: VisionModel[];
  canConfirmGl: boolean;
  glConfirmed: boolean;
}

function StepCard({
  waybillId,
  pip,
  pipIndexN,
  state,
  roleText,
  bucketKind,
  events,
  attachments,
  allowedKinds,
  isCurrentStage,
  canAct,
  canAttach,
  canSettle,
  canFinalApprove,
  originId,
  locale,
  approvers,
  currentUserId,
  visionModels,
  canConfirmGl,
  glConfirmed,
}: StepCardProps) {
  const tone = toneForState(state);
  const paysBefore = pip.paysBefore === true;
  const thirdParty = pip.thirdParty === true;
  const isRejected = state === 'rejected';
  const isPassed = state === 'passed';
  const isPending = state === 'pending' && !isCurrentStage;
  const isCollapsed = !isCurrentStage && state !== 'rejected';

  const rejectionEvent = isRejected
    ? events.find((e) => e.kind === 'rejected') ?? null
    : null;
  const lastAdvanced = lastAdvancedEvent(events, pip.key);

  if (isCollapsed) {
    return (
      <article
        id={`pip-${pip.key}`}
        className={'scroll-mt-24 flex items-center gap-3 rounded-md border px-4 py-2.5 transition ' + tone.card}
        aria-label={`${pip.label} · ${roleText}`}
      >
        <span className="text-xl leading-none" aria-hidden>{pip.emoji}</span>
        <Link
          href={`/waybill/${waybillId}?pip=${pip.key}`}
          className="flex min-w-0 flex-1 items-baseline gap-2"
          aria-label={`Open ${pip.label} detail`}
        >
          <span className={'truncate text-sm font-bold ' + tone.title}>
            <T id={pip.label} />
          </span>
          <span className="text-[10px] uppercase tracking-wider text-mute">
            #{pipIndexN + 1}
          </span>
          {lastAdvanced && (
            <span className="truncate text-xs text-mute">
              · {roleDisplay(lastAdvanced.actor_role, locale)} #{lastAdvanced.actor_id} · {formatDateServer(lastAdvanced.occurred_at, locale)}
            </span>
          )}
        </Link>
        <span
          className={'inline-flex shrink-0 items-center rounded-full px-2 py-0.5 text-[10px] font-bold uppercase ' + tone.badge}
        >
          <T id={PIP_BADGE_EN[state]} />
        </span>
      </article>
    );
  }

  return (
    <article
      id={`pip-${pip.key}`}
      className={'scroll-mt-24 rounded-md border p-4 transition ' + tone.card}
      aria-label={`${pip.label} · ${roleText}`}
    >
      <header className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <Link
          href={`/waybill/${waybillId}?pip=${pip.key}`}
          className="flex shrink-0 items-center gap-3"
           aria-label={`Open ${pip.label} detail`}
        >
          <span className="text-2xl leading-none" aria-hidden>{pip.emoji}</span>
          <h3 className={'text-base font-bold leading-tight sm:text-lg ' + tone.title}>
             <T id={pip.label} />
          </h3>
          <span className="text-xs uppercase tracking-wider text-[var(--text-faint)]">
            #{pipIndexN + 1}
          </span>
        </Link>
        <span
          className={'ml-auto inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-bold uppercase ' + tone.badge}
        >
           <T id={PIP_BADGE_EN[state]} />
        </span>
 <span className=" border bg-paper-2 border border-rule px-2 py-0.5 text-xs uppercase tracking-wider text-mute">
           <T id={bucketKind} />
        </span>
        {roleText && (
          <span className="inline-flex items-center gap-1 rounded-full border bg-info-soft border border-info/40 bg-info-soft border border-info/40 px-2 py-0.5 text-xs font-bold uppercase tracking-widest text-info">
             <T id="waybill.pip.role" />
             <span className="text-info"><T id={roleText} /></span>
          </span>
        )}
        {paysBefore && (
          <span className="inline-flex items-center gap-1 rounded-md border bg-caution-soft border border-caution/40 bg-caution-soft border border-caution/40 px-2 py-0.5 text-xs font-mono text-caution">
            🧾 <T id="waybill.timeline.pre_paid_slip" />
          </span>
        )}
        {thirdParty && (
          <span className="inline-flex items-center gap-1 rounded-md border bg-info-soft border border-info/40 bg-info-soft border border-info/40 px-2 py-0.5 text-xs font-mono text-info">
            💸 <T id="waybill.timeline.pays_3rd_party" />
          </span>
        )}
      </header>

      <p className={'mt-2 text-sm leading-snug italic ' + tone.sectionHead}>
         <T id={pip.description} />
      </p>

      <div className="mt-4 space-y-3">
        <DocumentsBlock
          attachments={attachments}
          waybillId={waybillId}
          sectionHead={tone.sectionHead}
        />

        <ActionPromptBlock
          state={state}
          isCurrentStage={isCurrentStage}
          isPassed={isPassed}
          isPending={isPending}
          isRejected={isRejected}
          canAct={canAct}
          canAttach={canAttach}
          canSettle={canSettle}
          canFinalApprove={canFinalApprove}
          originId={originId}
          allowedKinds={allowedKinds}
          roleText={roleText}
          rejectionEvent={rejectionEvent}
          lastAdvanced={lastAdvanced}
          waybillId={waybillId}
          pipKey={pip.key}
          locale={locale}
          approvers={approvers}
          currentUserId={currentUserId}
          visionModels={visionModels}
          events={events}
          canConfirmGl={canConfirmGl}
          glConfirmed={glConfirmed}
        />
      </div>
    </article>
  );
}

function DocumentsBlock({
  attachments,
  waybillId,
  sectionHead,
}: {
  attachments: WaybillAttachmentRow[];
  waybillId: string;
  sectionHead: string;
}) {
  return (
    <section>
      <div className={'text-xs uppercase tracking-widest ' + sectionHead}>
        {<T id="waybill.timeline.documents" />} ({attachments.length})
      </div>
      {attachments.length === 0 ? (
        <p className="mt-1.5 text-sm italic text-[var(--text-faint)]">
          {<T id="waybill.timeline.no_documents_at_this_pip" />}
        </p>
      ) : (
        <div className="mt-2 divide-y divide-rule/40">
          {attachments.map((a) => (
            <AttachmentRow key={a.id} waybillId={waybillId} attachment={a} />
          ))}
        </div>
      )}
    </section>
  );
}

function ActionPromptBlock({
  state: _state,
  isCurrentStage,
  isPassed,
  isPending,
  isRejected,
  canAct,
  canAttach,
  canSettle,
  canFinalApprove,
  originId,
  allowedKinds,
  roleText,
  rejectionEvent,
  lastAdvanced,
  waybillId,
  pipKey,
  locale,
  approvers,
  currentUserId,
  visionModels,
  events,
  canConfirmGl,
  glConfirmed,
}: {
  state: PipState;
  isCurrentStage: boolean;
  isPassed: boolean;
  isPending: boolean;
  isRejected: boolean;
  canAct: boolean;
  canAttach: boolean;
  canSettle: boolean;
  canFinalApprove: boolean;
  originId: number | null;
  allowedKinds: WaybillAttachmentKind[];
  roleText: string;
  rejectionEvent: WaybillEventRow | null;
  lastAdvanced: WaybillEventRow | null;
  waybillId: string;
  pipKey: string;
  locale: SecondaryLocale;
  approvers: ApproversByStage[string];
  currentUserId: number | null;
  visionModels: VisionModel[];
  events: WaybillEventRow[];
  canConfirmGl: boolean;
  glConfirmed: boolean;
}) {
  if (isRejected) {
    const reason =
      (rejectionEvent?.payload as { reason?: string } | null)?.reason ?? null;
    return (
      <section className="space-y-2 border-t bg-critical-soft border border-critical/40 pt-4 text-sm">
        <div className="text-xs uppercase tracking-widest text-critical/80">
          ✗ {<T id="waybill.timeline.rejected_by" />}
        </div>
        {rejectionEvent ? (
          <div className="mt-2 text-sm text-critical">
            <span className="font-mono">
              {roleDisplay(rejectionEvent.actor_role, locale)} #{rejectionEvent.actor_id ?? '—'}
            </span>
            <span className="ml-2 font-mono text-critical">{formatDateServer(rejectionEvent.occurred_at, locale)}</span>
            {reason && (
              <p className="mt-2 text-critical italic">&ldquo;{reason}&rdquo;</p>
            )}
          </div>
        ) : (
          <p className="mt-2 text-sm text-critical">
            {<T id="waybill.timeline.no_rejection_event_recorded" />}
          </p>
        )}
        <ApproversList approvers={approvers} locale={locale} tone="rose" currentUserId={null} />
      </section>
    );
  }

  if (isPassed) {
    return (
      <section className="space-y-2 border-t bg-positive-soft border border-positive/40 pt-4 text-sm">
        <div className="text-xs uppercase tracking-widest text-positive/80">
          ✓ {<T id="waybill.timeline.completed_by" />}
        </div>
        {lastAdvanced ? (
          <div className="mt-2 text-sm text-positive">
            <span className="font-mono">
              {roleDisplay(lastAdvanced.actor_role, locale)} #{lastAdvanced.actor_id ?? '—'}
            </span>
            <span className="ml-2 font-mono text-positive">{formatDateServer(lastAdvanced.occurred_at, locale)}</span>
          </div>
        ) : (
          <p className="mt-2 text-sm text-positive">
            {<T id="waybill.timeline.no_event_recorded_at_this_pip" />}
          </p>
        )}
        <ApproversList approvers={approvers} locale={locale} tone="emerald" currentUserId={null} />
      </section>
    );
  }

  if (isCurrentStage) {
    const isDisbursement = pipKey === 'awaiting_disbursement';
    const isDisbursed = pipKey === 'disbursed';
    const isFinalApproval = pipKey === 'final_authorization' || pipKey === 'accounting_authorization';
    if (isFinalApproval) {
      return (
        <section className="space-y-3 border-t border-accent pt-4 text-sm">
          <div className="flex flex-wrap items-baseline justify-between gap-2 text-xs uppercase tracking-widest text-accent">
            <span>
              🔒 {<T id="waybill.timeline.final_authorization" />}
              <span className="ml-2 text-mute">
                {<T id="waybill.timeline.current_stage" />}
              </span>
            </span>
            <span className="rounded-md border border-accent bg-accent px-2 py-0.5 text-accent">
              {<T id="waybill.timeline.approve_gl_post_reject_no_gl" />}
            </span>
          </div>
          {canAttach && (
            <AttachmentUpload waybillId={waybillId} stage={pipKey} />
          )}
          {canFinalApprove ? (
            <div className="grid gap-3 sm:grid-cols-2">
              <form action={finalApproveWaybillAction}>
                <input type="hidden" name="waybillId" value={waybillId} />
                <button
                  type="submit"
                  data-testid={`big-final-approve-${waybillId}`}
                  className="group inline-flex w-full flex-col items-center justify-center gap-1 rounded-md bg-positive px-5 py-5 text-lg font-bold text-ink shadow-popover shadow-positive transition hover:bg-positive-strong hover:shadow-positive-strong"
                >
                  <span className="flex items-center gap-2">
                    <span aria-hidden className="text-2xl">✓</span>
                    <span>{<T id="waybill.timeline.final_approve" />}</span>
                  </span>
                  <span className="text-xs uppercase tracking-widest text-positive-strong group-hover:text-positive-strong">
                    {<T id="waybill.timeline.posts_to_gl" />}
                  </span>
                </button>
              </form>
              <Link
                href={`/waybill/${waybillId}?action=final-reject&stage=${pipKey}`}
                data-testid={`big-final-reject-${waybillId}`}
                className="group inline-flex flex-col items-center justify-center gap-1 rounded-md bg-critical px-5 py-5 text-lg font-bold text-ink shadow-popover shadow-critical transition hover:bg-critical-strong hover:shadow-critical-strong"
              >
                <span className="flex items-center gap-2">
                  <span aria-hidden className="text-2xl">✗</span>
                  <span>{<T id="waybill.timeline.final_reject" />}</span>
                </span>
                <span className="text-xs uppercase tracking-widest text-critical-strong group-hover:text-critical-strong">
                  {<T id="waybill.timeline.no_gl_post" />}
                </span>
              </Link>
            </div>
          ) : (
            <p className="text-sm font-mono text-mute">
              {locale=== 'th'
                ? 'รอเจ้าหน้าที่การเงินอนุมัติขั้นสุดท้าย'
                : 'Waiting on a finance officer for the final approval.'}
            </p>
          )}
          <ApproversList
            approvers={approvers}
            locale={locale}
            tone="cyan"
            currentUserId={currentUserId}
          />
        </section>
      );
    }
    if (isDisbursed) {
      const confirmEvent = events
        .filter((e) => e.kind === 'gl-confirmed')
        .sort((a, b) => b.sequence - a.sequence)[0];
      return (
        <section className="space-y-3 border-t bg-positive-soft border border-positive/40 pt-4 text-sm">
          <div className="text-xs uppercase tracking-widest text-positive">
            ✅ {<T id="waybill.timeline.confirm_gl_recorded" />}
            <span className="ml-2 text-mute">
              {<T id="waybill.timeline.current_stage" />}
            </span>
          </div>
          {glConfirmed && confirmEvent ? (
            <div className="rounded-md border bg-positive-soft border border-positive/40 bg-positive-soft border border-positive/40 p-3 text-sm text-positive">
              <div className="font-mono">
                ✓ {<T id="waybill.timeline.confirmed_by" />}{' '}
                {roleDisplay(confirmEvent.actor_role, locale)} #{confirmEvent.actor_id ?? '—'}
              </div>
              <div className="mt-1 font-mono text-positive">{formatDateServer(confirmEvent.occurred_at, locale)}</div>
            </div>
          ) : canConfirmGl && originId != null ? (
            <form
              action={confirmGlRecordedAction}
              className="rounded-md border border-caution bg-caution-soft p-4 text-sm text-caution"
            >
              <input type="hidden" name="waybillId" value={waybillId} />
              <input type="hidden" name="expenseId" value={originId} />
              <p className="mb-3 font-mono">
                {locale=== 'th'
                  ? 'บัญชีแล้ว — เจ้าหน้าที่บัญชี/การเงินท่านใดกดยืนยันก็ได้'
                  : 'GL posted — any accounting/finance officer can click to confirm.'}
              </p>
              <button
                type="submit"
                data-testid={`gl-confirm-${waybillId}`}
                className="inline-flex w-full items-center justify-center gap-2 rounded-md bg-caution px-5 py-5 text-lg font-bold text-ink shadow-popover shadow-caution transition hover:bg-caution-strong hover:shadow-caution-strong"
              >
                <span aria-hidden className="text-2xl">✓</span>
                <span>{<T id="waybill.timeline.confirm_gl_recorded" />}</span>
              </button>
            </form>
          ) : (
            <p className="text-sm font-mono text-mute">
              {locale=== 'th'
                ? 'รอการบันทึกบัญชี (GL) จากขั้นตอนก่อนหน้า'
                : 'Waiting for the GL post from the previous step.'}
            </p>
          )}
          <ApproversList
            approvers={approvers}
            locale={locale}
            tone="emerald"
            currentUserId={currentUserId}
          />
        </section>
      );
    }
    if (isDisbursement && canSettle && originId != null) {
      return (
        <section className="space-y-3 border-t bg-info-soft border border-info/40 pt-4 text-sm">
          <div className="text-xs uppercase tracking-widest text-info">
            💸 {<T id="waybill.timeline.disbursement_step" />}
            <span className="ml-2 text-mute">
              {<T id="waybill.timeline.current_stage" />}
            </span>
          </div>
          <SettleForm waybillId={waybillId} expenseId={originId} visionModels={visionModels} />
          <ApproversList
            approvers={approvers}
            locale={locale}
            tone="cyan"
            currentUserId={currentUserId}
          />
        </section>
      );
    }
    return (
      <section className="space-y-3 border-t bg-info-soft border border-info/40 pt-4 text-sm">
        <div className="text-xs uppercase tracking-widest text-info">
          ⚡ {<T id="waybill.timeline.acting_now" />}
          <span className="ml-2 text-mute">
            {<T id="waybill.timeline.current_stage" />}
          </span>
        </div>
        {canAttach && (
          <AttachmentUpload waybillId={waybillId} stage={pipKey} />
        )}
        {canAct ? (
          <div className="grid gap-3 sm:grid-cols-2">
            <form action={approveWaybillAction}>
              <input type="hidden" name="waybillId" value={waybillId} />
              <button
                type="submit"
                data-testid={`big-approve-${pipKey}`}
                className="inline-flex w-full items-center justify-center gap-2 rounded-md bg-positive px-5 py-5 text-lg font-bold text-ink shadow-popover shadow-positive transition hover:bg-positive-strong hover:shadow-positive-strong"
              >
                <span aria-hidden className="text-2xl">✓</span>
                <span>{<T id="waybill.timeline.approve" />}</span>
              </button>
            </form>
            <Link
              href={`/waybill/${waybillId}?action=reject&stage=${pipKey}`}
              data-testid={`big-reject-${pipKey}`}
              className="inline-flex items-center justify-center gap-2 rounded-md bg-critical px-5 py-5 text-lg font-bold text-ink shadow-popover shadow-critical transition hover:bg-critical-strong hover:shadow-critical-strong"
            >
              <span aria-hidden className="text-2xl">✗</span>
              <span>{<T id="waybill.timeline.reject" />}</span>
            </Link>
          </div>
        ) : (
          <p className="text-sm font-mono text-mute">
            {locale=== 'th'
              ? 'คุณไม่มีสิทธิ์อนุมัติขั้นนี้'
              : "you don't have approval rights for this stage"}
          </p>
        )}
        <ApproversList
          approvers={approvers}
          locale={locale}
          tone="cyan"
          currentUserId={currentUserId}
        />
      </section>
    );
  }

  if (isPending) {
    return (
      <section className="space-y-3 border-t bg-info-soft border border-info/40 pt-4 text-sm">
        <div className="text-xs uppercase tracking-widest text-info">
          🪜 {<T id="waybill.timeline.acting_next" />}
          <span className="ml-2 text-mute">
            {<T id="waybill.timeline.read_only_preview" />}
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {roleText && (
            <span className="inline-flex items-center gap-1 rounded-md border bg-info-soft border border-info/40 bg-info-soft border border-info/40 px-2 py-1 text-xs font-mono text-info">
              {<T id="waybill.timeline.will_act_as" />}{' '}
               <span className="font-bold text-info"><T id={roleText} /></span>
            </span>
          )}
        </div>
        <div>
          <div className="text-xs uppercase tracking-widest text-[var(--text-faint)]">
             <T id="waybill.timeline.attachmentsNextApprover" />
          </div>
          {allowedKinds.length === 0 ? (
            <p className="mt-2 text-sm italic text-[var(--text-faint)]">
               <T id="waybill.timeline.noAttachmentsStage" />
            </p>
          ) : (
            <ul className="mt-2 space-y-1">
              {allowedKinds.map((k) => {
                const meta = WAYBILL_KINDS[k];
                return (
                  <li
                    key={k}
 className="flex items-center gap-2 border bg-paper-2 border border-rule px-2.5 py-1 text-xs"
                  >
                    <span aria-hidden className="text-base">{meta.emoji}</span>
                    <span className="font-mono text-info">{k}</span>
                     <span className="text-mute">— <T id={meta.id} /></span>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
        <ApproversList
          approvers={approvers}
          locale={locale}
          tone="cyan"
          currentUserId={currentUserId}
        />
      </section>
    );
  }

  return null;
}

