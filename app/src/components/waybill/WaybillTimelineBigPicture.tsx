import React from 'react';
import Link from 'next/link';
import type { WaybillDomain, WaybillStagePip, PipState } from '@folio-lib/waybill/derive';
import type { SecondaryLocale } from '@folio-lib/server/locale';
import {
  pipsForDomain,
  pipIndex,
  bucketLabel,
  computePipState,
  stageRoleLabel,
} from '@folio-lib/waybill/derive';
import type { WaybillEventRow } from '@folio-lib/waybill/events';
import type { WaybillAttachmentRow } from '@folio-lib/waybill/attachments';
import {
  WAYBILL_KINDS,
  allowedKindsFor,
  type WaybillAttachmentKind,
} from '@folio-lib/waybill/kinds';
import { AttachmentRow } from './AttachmentRow';
import { AttachmentUpload } from './AttachmentUpload';
import type { ApproversByStage } from '@folio-lib/waybill/queries';
import { approveWaybillAction, confirmGlRecordedAction, finalApproveWaybillAction } from '@/app/actions';
import { SettleForm } from '@/app/(app)/(protected)/waybill/[id]/_components/SettleForm';
import type { VisionModel } from '@folio-lib/ai/loadVisionModels';
import { roleDisplay, eventKindLabel } from './ui';
import { formatDateServer } from '@/components/i18n/formattersServer';
import { Bilingual } from '@/components/i18n/Bilingual';
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
  passed: 'Done',
  active: 'Active',
  pending: 'Pending',
  rejected: 'Rejected',
  skipped: 'Optional',
};
const PIP_BADGE_TH: Record<PipState, string> = {
  passed: 'ผ่าน',
  active: 'กำลังดำเนินการ',
  pending: 'รอ',
  rejected: 'ปฏิเสธ',
  skipped: 'ไม่บังคับ',
};
const PIP_BADGE_DE: Record<PipState, string> = {
  passed: 'Erledigt',
  active: 'Aktiv',
  pending: 'Ausstehend',
  rejected: 'Abgelehnt',
  skipped: 'Optional',
};

const BULLET_GLYPH: Record<PipState, string> = {
  passed: '✓',
  active: '◉',
  pending: '○',
  rejected: '✗',
  skipped: '—',
};

function toneForState(state: PipState) {
  switch (state) {
    case 'passed':
      return {
        card: 'border-emerald-500/50 bg-emerald-950/30 text-emerald-100',
        bullet: 'bg-emerald-400 shadow-[0_0_12px_rgba(52,211,153,0.7)] text-emerald-200',
        badge: 'bg-emerald-500/20 text-emerald-200 border border-emerald-400/40',
        title: 'text-emerald-200',
        sectionHead: 'text-emerald-300/80',
      };
    case 'active':
      return {
        card: 'border-cyan-400/80 bg-cyan-950/40 text-white ring-2 ring-cyan-400/70',
        bullet: 'bg-cyan-300 shadow-[0_0_14px_rgba(34,211,238,0.9)] animate-pulse text-cyan-100',
        badge: 'bg-cyan-400 text-slate-950 border border-cyan-300',
        title: 'text-cyan-200',
        sectionHead: 'text-cyan-300/80',
      };
    case 'skipped':
      return {
        card: 'border-slate-900 bg-slate-950/30 text-slate-700 opacity-60',
        bullet: 'bg-slate-800 text-slate-600',
        badge: 'bg-slate-800 text-slate-600 border border-slate-700',
        title: 'text-slate-700',
        sectionHead: 'text-slate-700',
      };
    case 'rejected':
      return {
        card: 'border-rose-500/70 bg-rose-950/40 text-rose-100 ring-2 ring-rose-500/50',
        bullet: 'bg-rose-400 text-rose-100 shadow-[0_0_10px_rgba(244,63,94,0.7)]',
        badge: 'bg-rose-500/25 text-rose-200 border border-rose-400/50',
        title: 'text-rose-200',
        sectionHead: 'text-rose-300/80',
      };
    case 'pending':
    default:
      return {
        card: 'border-slate-800/60 bg-slate-950/40 text-slate-400',
        bullet: 'bg-slate-700 text-slate-500',
        badge: 'bg-slate-800 text-slate-500 border border-slate-700',
        title: 'text-slate-400',
        sectionHead: 'text-slate-500',
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
          <span aria-hidden className="grid h-9 w-9 place-items-center rounded-xl bg-gradient-to-br from-cyan-500/30 to-indigo-500/30 text-lg ring-1 ring-cyan-400/40">
            🚦
          </span>
          <div className="flex flex-col">
            <span className="text-base font-bold text-white sm:text-lg">
              {domain === 'procurement' ? 'Procurement Pipeline' : 'Expense Pipeline'}
            </span>
            <span className="text-xs font-mono uppercase tracking-widest text-slate-500">
              {<Bilingual en="bottom → top (approval order)" th="ล่าง→บน (ลำดับอนุมัติ)" locale={locale} />}
            </span>
          </div>
        </div>
        <span className="rounded-full border border-slate-700 bg-slate-900/60 px-3 py-1 text-xs font-mono text-slate-300">
          {isRejected
            ? <Bilingual en="Closed (rejected)" th="ปิดแล้ว (ปฏิเสธ)" locale={locale} />
            : `${<Bilingual en="step" th="ขั้น" locale={locale} />} ${curIdx < 0 ? items.length : curIdx + 1} ${<Bilingual en="of" th="จาก" locale={locale} />} ${items.length}`}
        </span>
      </header>

      <div className="flex items-stretch gap-5">
        <div className="relative flex w-8 shrink-0 flex-col items-center pt-2 pb-2">
          <span aria-hidden className="z-10 mb-1 text-lg leading-none text-cyan-300 drop-shadow-[0_0_8px_rgba(34,211,238,0.7)]">▲</span>
          <span className="z-10 h-1 w-4 rounded bg-cyan-400" />
          <div className="relative z-0 h-full w-1 flex-1 rounded-full bg-gradient-to-b from-cyan-400 via-indigo-500/60 to-slate-800" />
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
  const bullet = BULLET_GLYPH[state];
  const isRejected = state === 'rejected';
  const isPassed = state === 'passed';
  const isPending = state === 'pending' && !isCurrentStage;

  const rejectionEvent = isRejected
    ? events.find((e) => e.kind === 'rejected') ?? null
    : null;
  const lastAdvanced = isPassed
    ? events.filter((e) => e.kind === 'advanced' && e.stage_to === pip.key).slice(-1)[0] ??
      events.filter((e) => e.stage_to === pip.key).slice(-1)[0] ??
      null
    : null;

  return (
    <article
      className={'rounded-2xl border p-4 transition ' + tone.card}
      aria-label={`${pip.en} · ${locale === 'de' ? pip.de : pip.th} · ${roleText}`}
    >
      <header className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <Link
          href={`/waybill/${waybillId}?pip=${pip.key}`}
          className="flex shrink-0 items-center gap-3"
          aria-label={`Open ${pip.en} · ${locale === 'de' ? pip.de : pip.th} detail`}
        >
          <span
            className={'flex h-8 w-8 items-center justify-center rounded-full text-sm font-bold ' + tone.bullet}
            aria-hidden
          >
            {bullet}
          </span>
          <span className="text-2xl leading-none" aria-hidden>{pip.emoji}</span>
          <h3 className={'text-base font-bold leading-tight sm:text-lg ' + tone.title}>
            <Bilingual en={pip.en} th={pip.th} de={pip.de} locale={locale} />
          </h3>
          <span className="font-mono text-xs uppercase tracking-wider text-slate-500">
            #{pipIndexN + 1}
          </span>
        </Link>
        <span
          className={'ml-auto inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-bold uppercase ' + tone.badge}
        >
          <Bilingual en={PIP_BADGE_EN[state]} th={PIP_BADGE_TH[state]} de={PIP_BADGE_DE[state]} locale={locale} />
        </span>
        <span className="rounded-md border border-slate-700/60 bg-slate-900/60 px-2 py-0.5 text-xs font-mono uppercase tracking-wider text-slate-400">
          {bucketKind}
        </span>
        {roleText && (
          <span className="inline-flex items-center gap-1 rounded-full border border-cyan-500/40 bg-cyan-500/15 px-2 py-0.5 text-xs font-mono font-bold uppercase tracking-widest text-cyan-200">
            <span className="text-cyan-300/80">role:</span>
            <span className="text-cyan-100">{roleText}</span>
          </span>
        )}
        {paysBefore && (
          <span className="inline-flex items-center gap-1 rounded-md border border-amber-500/40 bg-amber-500/15 px-2 py-0.5 text-xs font-mono text-amber-200">
            🧾 <Bilingual en="Pre-paid slip" th="จ่ายก่อนได้ (แนบสลิป)" de="Vorausbezahlter Beleg" locale={locale} />
          </span>
        )}
        {thirdParty && (
          <span className="inline-flex items-center gap-1 rounded-md border border-indigo-500/40 bg-indigo-500/15 px-2 py-0.5 text-xs font-mono text-indigo-200">
            💸 <Bilingual en="Pays 3rd party" th="จ่ายบุคคลที่สาม" de="Zahlung an Dritte" locale={locale} />
          </span>
        )}
      </header>

      <p className={'mt-2 text-sm leading-snug italic ' + tone.sectionHead}>
        <Bilingual en={pip.description_en} th={pip.description_th} locale={locale} />
      </p>

      <div className="mt-4 space-y-3">
        <ActivityBlock
          state={state}
          events={events}
          locale={locale}
          sectionHead={tone.sectionHead}
        />

        <DocumentsBlock
          state={state}
          attachments={attachments}
          waybillId={waybillId}
          locale={locale}
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

function ActivityBlock({
  state,
  events,
  locale,
  sectionHead,
}: {
  state: PipState;
  events: WaybillEventRow[];
  locale: SecondaryLocale;
  sectionHead: string;
}) {
  const stateAccent =
    state === 'passed'
      ? 'text-emerald-200/80'
      : state === 'active'
      ? 'text-cyan-200/80'
      : state === 'rejected'
      ? 'text-rose-200/80'
      : 'text-slate-500';

  return (
    <section>
      <div className={'text-xs font-mono uppercase tracking-widest ' + sectionHead}>
        {<Bilingual en="Activity" th="กิจกรรม" locale={locale} />} ({events.length})
      </div>
      {events.length === 0 ? (
        <p className={'mt-1.5 text-sm italic ' + stateAccent}>
          {<Bilingual en="no events recorded at this pip yet" th="ยังไม่มีเหตุการณ์" locale={locale} />}
        </p>
      ) : (
        <ol className="mt-2 divide-y divide-slate-800/40">
          {events.map((e) => (
            <li
              key={e.id}
              className="py-2 text-xs font-mono text-slate-300"
            >
              <span className="text-cyan-400">#{e.sequence}</span>
              <span className="mx-2 text-slate-700">·</span>
              <span className="font-bold text-white">{eventKindLabel(e.kind, locale)}</span>
              {e.stage_from || e.stage_to ? (
                <span className="ml-2 text-slate-400">
                  {e.stage_from ?? '—'} → <span className="text-cyan-300">{e.stage_to ?? '—'}</span>
                </span>
              ) : null}
              {e.actor_id != null && (
                <span className="ml-2 text-slate-500">
                  by {roleDisplay(e.actor_role, locale)} <span className="text-slate-400">#{e.actor_id}</span>
                </span>
              )}
              <span className="ml-2 text-slate-500">{formatDateServer(e.occurred_at, locale)}</span>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

function DocumentsBlock({
  attachments,
  waybillId,
  locale,
  sectionHead,
}: {
  state: PipState;
  attachments: WaybillAttachmentRow[];
  waybillId: string;
  locale: SecondaryLocale;
  sectionHead: string;
}) {
  return (
    <section>
      <div className={'text-xs font-mono uppercase tracking-widest ' + sectionHead}>
        {<Bilingual en="Documents" th="เอกสาร" locale={locale} />} ({attachments.length})
      </div>
      {attachments.length === 0 ? (
        <p className="mt-1.5 text-sm italic text-slate-500">
          {<Bilingual en="no documents at this pip" th="ไม่มีเอกสาร" locale={locale} />}
        </p>
      ) : (
        <div className="mt-2 divide-y divide-slate-800/40">
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
      <section className="space-y-2 border-t border-rose-500/30 pt-4 text-sm">
        <div className="text-xs font-mono uppercase tracking-widest text-rose-200/80">
          ✗ {<Bilingual en="Rejected by" th="ปฏิเสธโดย" locale={locale} />}
        </div>
        {rejectionEvent ? (
          <div className="mt-2 text-sm text-rose-100">
            <span className="font-mono">
              {roleDisplay(rejectionEvent.actor_role, locale)} #{rejectionEvent.actor_id ?? '—'}
            </span>
            <span className="ml-2 font-mono text-rose-300">{formatDateServer(rejectionEvent.occurred_at, locale)}</span>
            {reason && (
              <p className="mt-2 text-rose-100 italic">&ldquo;{reason}&rdquo;</p>
            )}
          </div>
        ) : (
          <p className="mt-2 text-sm text-rose-100">
            {<Bilingual en="no rejection event recorded" th="ไม่มีเหตุการณ์การปฏิเสธ" locale={locale} />}
          </p>
        )}
        <ApproversList approvers={approvers} locale={locale} tone="rose" currentUserId={null} />
      </section>
    );
  }

  if (isPassed) {
    return (
      <section className="space-y-2 border-t border-emerald-500/30 pt-4 text-sm">
        <div className="text-xs font-mono uppercase tracking-widest text-emerald-200/80">
          ✓ {<Bilingual en="Completed by" th="เสร็จสิ้นโดย" locale={locale} />}
        </div>
        {lastAdvanced ? (
          <div className="mt-2 text-sm text-emerald-100">
            <span className="font-mono">
              {roleDisplay(lastAdvanced.actor_role, locale)} #{lastAdvanced.actor_id ?? '—'}
            </span>
            <span className="ml-2 font-mono text-emerald-300">{formatDateServer(lastAdvanced.occurred_at, locale)}</span>
          </div>
        ) : (
          <p className="mt-2 text-sm text-emerald-100">
            {<Bilingual en="no event recorded at this pip" th="ไม่มีเหตุการณ์ที่บันทึกในขั้นนี้" locale={locale} />}
          </p>
        )}
        <ApproversList approvers={approvers} locale={locale} tone="emerald" currentUserId={null} />
      </section>
    );
  }

  if (isCurrentStage) {
    const isDisbursement = pipKey === 'awaiting_disbursement';
    const isDisbursed = pipKey === 'disbursed';
    const isFinalApproval = pipKey === 'final_authorization';
    if (isFinalApproval) {
      return (
        <section className="space-y-3 border-t border-fuchsia-500/30 pt-4 text-sm">
          <div className="flex flex-wrap items-baseline justify-between gap-2 text-xs font-mono uppercase tracking-widest text-fuchsia-200">
            <span>
              🔒 {<Bilingual en="Final Authorization" th="อนุมัติขั้นสุดท้าย" locale={locale} />}
              <span className="ml-2 text-slate-400">
                {<Bilingual en="current stage" th="ขั้นปัจจุบัน" locale={locale} />}
              </span>
            </span>
            <span className="rounded-md border border-fuchsia-400/40 bg-fuchsia-500/15 px-2 py-0.5 text-fuchsia-200">
              {<Bilingual en="approve = GL post · reject = no GL" th="อนุมัติ = บันทึกบัญชี · ปฏิเสธ = ไม่บันทึก" locale={locale} />}
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
                  className="group inline-flex w-full flex-col items-center justify-center gap-1 rounded-2xl bg-gradient-to-br from-emerald-400 to-cyan-500 px-5 py-5 text-lg font-bold text-slate-950 shadow-xl shadow-emerald-500/40 transition hover:from-emerald-300 hover:to-cyan-400 hover:shadow-emerald-500/50"
                >
                  <span className="flex items-center gap-2">
                    <span aria-hidden className="text-2xl">✓</span>
                    <span>{<Bilingual en="Final approve" th="อนุมัติขั้นสุดท้าย" locale={locale} />}</span>
                  </span>
                  <span className="text-xs font-mono uppercase tracking-widest text-emerald-950/70 group-hover:text-emerald-950">
                    {<Bilingual en="→ posts to GL" th="→ บันทึกบัญชี (GL)" locale={locale} />}
                  </span>
                </button>
              </form>
              <Link
                href={`/waybill/${waybillId}?action=final-reject&stage=final_authorization`}
                data-testid={`big-final-reject-${waybillId}`}
                className="group inline-flex flex-col items-center justify-center gap-1 rounded-2xl bg-gradient-to-br from-rose-400 to-rose-600 px-5 py-5 text-lg font-bold text-slate-950 shadow-xl shadow-rose-500/40 transition hover:from-rose-300 hover:to-rose-500 hover:shadow-rose-500/50"
              >
                <span className="flex items-center gap-2">
                  <span aria-hidden className="text-2xl">✗</span>
                  <span>{<Bilingual en="Final reject" th="ปฏิเสธขั้นสุดท้าย" locale={locale} />}</span>
                </span>
                <span className="text-xs font-mono uppercase tracking-widest text-rose-950/70 group-hover:text-rose-950">
                  {<Bilingual en="→ no GL post" th="→ ไม่บันทึกบัญชี" locale={locale} />}
                </span>
              </Link>
            </div>
          ) : (
            <p className="text-sm font-mono text-slate-400">
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
        <section className="space-y-3 border-t border-emerald-500/30 pt-4 text-sm">
          <div className="text-xs font-mono uppercase tracking-widest text-emerald-300">
            ✅ {<Bilingual en="Confirm GL recorded" th="ยืนยันการบันทึกบัญชี" locale={locale} />}
            <span className="ml-2 text-slate-400">
              {<Bilingual en="current stage" th="ขั้นปัจจุบัน" locale={locale} />}
            </span>
          </div>
          {glConfirmed && confirmEvent ? (
            <div className="rounded-xl border border-emerald-500/30 bg-emerald-500/10 p-3 text-sm text-emerald-100">
              <div className="font-mono">
                ✓ {<Bilingual en="Confirmed by" th="ยืนยันโดย" locale={locale} />}{' '}
                {roleDisplay(confirmEvent.actor_role, locale)} #{confirmEvent.actor_id ?? '—'}
              </div>
              <div className="mt-1 font-mono text-emerald-300">{formatDateServer(confirmEvent.occurred_at, locale)}</div>
            </div>
          ) : canConfirmGl && originId != null ? (
            <form
              action={confirmGlRecordedAction}
              className="rounded-xl border border-amber-500/50 bg-amber-950/30 p-4 text-sm text-amber-100"
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
                className="inline-flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-br from-amber-400 to-amber-500 px-5 py-5 text-lg font-bold text-slate-950 shadow-lg shadow-amber-500/40 transition hover:from-amber-300 hover:to-amber-400 hover:shadow-amber-500/50"
              >
                <span aria-hidden className="text-2xl">✓</span>
                <span>{<Bilingual en="Confirm GL recorded" th="ยืนยันการบันทึกบัญชี" locale={locale} />}</span>
              </button>
            </form>
          ) : (
            <p className="text-sm font-mono text-slate-400">
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
        <section className="space-y-3 border-t border-cyan-500/30 pt-4 text-sm">
          <div className="text-xs font-mono uppercase tracking-widest text-cyan-300">
            💸 {<Bilingual en="Disbursement step" th="ขั้นตอนการจ่ายเงิน" locale={locale} />}
            <span className="ml-2 text-slate-400">
              {<Bilingual en="current stage" th="ขั้นปัจจุบัน" locale={locale} />}
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
      <section className="space-y-3 border-t border-cyan-500/30 pt-4 text-sm">
        <div className="text-xs font-mono uppercase tracking-widest text-cyan-300">
          ⚡ {<Bilingual en="Acting now?" th="กำลังดำเนินการ" locale={locale} />}
          <span className="ml-2 text-slate-400">
            {<Bilingual en="current stage" th="ขั้นปัจจุบัน" locale={locale} />}
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
                className="inline-flex w-full items-center justify-center gap-2 rounded-2xl bg-gradient-to-br from-emerald-400 to-emerald-600 px-5 py-5 text-lg font-bold text-slate-950 shadow-xl shadow-emerald-500/40 transition hover:from-emerald-300 hover:to-emerald-500 hover:shadow-emerald-500/50"
              >
                <span aria-hidden className="text-2xl">✓</span>
                <span>{<Bilingual en="Approve" th="อนุมัติ" locale={locale} />}</span>
              </button>
            </form>
            <Link
              href={`/waybill/${waybillId}?action=reject&stage=${pipKey}`}
              data-testid={`big-reject-${pipKey}`}
              className="inline-flex items-center justify-center gap-2 rounded-2xl bg-gradient-to-br from-rose-400 to-rose-600 px-5 py-5 text-lg font-bold text-slate-950 shadow-xl shadow-rose-500/40 transition hover:from-rose-300 hover:to-rose-500 hover:shadow-rose-500/50"
            >
              <span aria-hidden className="text-2xl">✗</span>
              <span>{<Bilingual en="Reject" th="ปฏิเสธ" locale={locale} />}</span>
            </Link>
          </div>
        ) : (
          <p className="text-sm font-mono text-slate-400">
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
      <section className="space-y-3 border-t border-cyan-500/30 pt-4 text-sm">
        <div className="text-xs font-mono uppercase tracking-widest text-cyan-300">
          🪜 {<Bilingual en="Acting next?" th="ขั้นตอนถัดไป" locale={locale} />}
          <span className="ml-2 text-slate-400">
            {<Bilingual en="read-only preview" th="อ่านอย่างเดียว" locale={locale} />}
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {roleText && (
            <span className="inline-flex items-center gap-1 rounded-md border border-cyan-500/40 bg-cyan-500/10 px-2 py-1 text-xs font-mono text-cyan-200">
              {<Bilingual en="will act as" th="จะดำเนินการโดย" locale={locale} />}{' '}
              <span className="font-bold text-cyan-100">{roleText}</span>
            </span>
          )}
        </div>
        <div>
          <div className="text-xs font-mono uppercase tracking-widest text-slate-500">
            {locale=== 'th'
              ? 'เอกสารแนบที่ผู้อนุมัติถัดไปสามารถเพิ่มได้'
              : 'Required attachments the next approver may add'}
          </div>
          {allowedKinds.length === 0 ? (
            <p className="mt-2 text-sm italic text-slate-500">
              {locale=== 'th'
                ? 'ผู้อนุมัตินี้ไม่ต้องแนบเอกสาร'
                : 'this approver adds no documents at this stage'}
            </p>
          ) : (
            <ul className="mt-2 space-y-1">
              {allowedKinds.map((k) => {
                const meta = WAYBILL_KINDS[k];
                return (
                  <li
                    key={k}
                    className="flex items-center gap-2 rounded-lg border border-slate-800/60 bg-slate-950/40 px-2.5 py-1 text-xs"
                  >
                    <span aria-hidden className="text-base">{meta.emoji}</span>
                    <span className="font-mono text-cyan-300">{k}</span>
                    <span className="text-slate-400">— <T value={{ en: meta.en, th: meta.th, de: meta.de }} /></span>
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

