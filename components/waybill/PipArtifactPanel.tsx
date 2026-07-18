import React from 'react';
import { GlVisibilityGate } from './GlVisibilityGate';
import type { SecondaryLocale } from '@/server/locale';
import type { JournalLineRow } from '@/waybill/queries';
import { formatDateServer } from '@/components/i18n/formattersServer';
import { T } from '@/components/i18n/TServer';

export type ArtifactKind = 'pr' | 'po' | 'gl-accrual' | 'gl-settlement' | 'paySlip';

interface ArtifactShape {
  id: number | string | null;
  status: string | null;
  href: string | null;
  finalizedAt?: Date | null;
  finalizedByName?: string | null;
  lines?: JournalLineRow[];
}

interface Props {
  kind: ArtifactKind;
  waybillId: string;
  artifact: ArtifactShape | null;
  canApprove: boolean;
  approveAction?: ((formData: FormData) => Promise<void> | void) | undefined;
  approveHiddenInputs?: Record<string, string>;
  approveLabel: string;
  actorCanSeeLines: boolean;
  locale?: SecondaryLocale;
  disabledReason?: string | null;
}

const KIND_META: Record<ArtifactKind, { emoji: string; tone: string }> = {
  pr:             { emoji: '📄', tone: 'border-info bg-info-strong' },
  po:             { emoji: '📦', tone: 'border-caution bg-caution-strong' },
  'gl-accrual':   { emoji: '📒', tone: 'border-info bg-info-strong' },
  'gl-settlement':{ emoji: '📒', tone: 'border-caution bg-caution-strong' },
  paySlip:        { emoji: '💳', tone: 'border-positive bg-positive-strong' },
};

function StatusPill({ status }: { status: string | null }) {
  const map: Record<string, string> = {
    draft: 'bg-paper-2 text-ink border-rule',
    submission: 'bg-info text-paper border-info',
    accounting_authorization: 'bg-caution-soft text-caution-strong border border-caution border-caution',
    cfo_authorization: 'bg-caution-soft text-caution-strong border border-caution border-caution',
    awaiting_disbursement: 'bg-accent text-paper border-accent',
    finalized: 'bg-positive text-paper border-positive',
    issued: 'bg-positive text-paper border-positive',
    settled: 'bg-positive text-paper border-positive',
    confirmed: 'bg-positive text-paper border-positive',
    approved: 'bg-positive text-paper border-positive',
    rejected: 'bg-critical-soft text-critical-strong border border-critical border-critical',
  };
  const cls = map[status ?? ''] ?? 'bg-paper-2 text-ink border-rule';
  return (
    <span className={`rounded-full border px-2 py-0.5 text-xs font-mono uppercase tracking-wider ${cls}`}>
      {status ?? '—'}
    </span>
  );
}

async function LinesInline({
  lines,
  actorCanSeeLines,
  locale,
}: {
  lines: JournalLineRow[];
  actorCanSeeLines: boolean;
  locale: SecondaryLocale;
}) {
  const totalDebit = lines.reduce((s, l) => s + l.debit, 0);
  const totalCredit = lines.reduce((s, l) => s + l.credit, 0);
  const balanced = Math.abs(totalDebit - totalCredit) < 0.005;
  const intlLocale = locale === 'de' ? 'de-DE' : 'th-TH';
  const fmtNum = (n: number) => n.toLocaleString(intlLocale, { minimumFractionDigits: 2 });
  return (
    <GlVisibilityGate
      actorCanSeeLines={actorCanSeeLines}
      totalDebit={totalDebit}
      totalCredit={totalCredit}
      balanced={balanced}
      lineCount={lines.length}
    >
      <table className="w-full min-w-[420px] border-collapse text-sm">
        <thead>
          <tr className="text-left font-mono text-xs uppercase tracking-widest text-mute">
            <th className="border-b border-rule/60 px-2 py-1">
              <T id="waybill.gl.code" locale={locale} />
            </th>
            <th className="border-b border-rule/60 px-2 py-1">
              <T id="waybill.gl.account" locale={locale} />
            </th>
            <th className="border-b border-rule/60 px-2 py-1 text-right">
              <T id="waybill.gl.debit" locale={locale} />
            </th>
            <th className="border-b border-rule/60 px-2 py-1 text-right">
              <T id="waybill.gl.credit" locale={locale} />
            </th>
          </tr>
        </thead>
        <tbody>
          {lines.map((l, i) => (
            <tr key={i} className="align-top font-mono text-ink-2">
              <td className="border-b border-rule/40 px-2 py-1 text-info">{l.account_code}</td>
              <td className="border-b border-rule/40 px-2 py-1 text-ink">
                {locale === 'th' ? l.account_name_th ?? l.account_name ?? '—' : l.account_name ?? l.account_name_th ?? '—'}
              </td>
              <td className="border-b border-rule/40 px-2 py-1 text-right text-positive-soft tabular-nums">
                {l.debit > 0 ? fmtNum(l.debit) : ''}
              </td>
              <td className="border-b border-rule/40 px-2 py-1 text-right text-caution-soft tabular-nums">
                {l.credit > 0 ? fmtNum(l.credit) : ''}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </GlVisibilityGate>
  );
}

export async function PipArtifactPanel({
  kind,
  waybillId: _waybillId,
  artifact,
  canApprove,
  approveAction,
  approveHiddenInputs,
  approveLabel,
  actorCanSeeLines,
  locale,
  disabledReason,
}: Props) {
  const localeSafe: SecondaryLocale = locale ?? 'th';
  const meta = KIND_META[kind];
  const showLines = (kind === 'gl-accrual' || kind === 'gl-settlement') && (artifact?.lines?.length ?? 0) > 0;
  const finalizedDateLabel = artifact?.finalizedAt
    ? await formatDateServer(artifact.finalizedAt, localeSafe)
    : null;
  return (
    <div className={`rounded-md border p-4 ${meta.tone}`}>
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="flex items-center gap-2">
          <span aria-hidden className="text-lg leading-none">{meta.emoji}</span>
          <span className="text-sm font-bold text-ink">
            <T id={`waybill.pip.${kind === 'paySlip' ? 'paymentSlip' : kind === 'pr' ? 'openPr' : kind === 'po' ? 'openPo' : kind === 'gl-accrual' ? 'glBeforePay' : 'glAfterPay'}`} locale={localeSafe} />
          </span>
        </div>
        <StatusPill status={artifact?.status ?? null} />
      </div>
      <div className="mt-2 font-mono text-base text-info-soft">
        {artifact?.id ?? '—'}
      </div>
      {artifact?.finalizedAt && (
        <div className="mt-1 text-xs font-mono text-mute">
          finalized: {finalizedDateLabel}
          {artifact.finalizedByName ? ` · ${artifact.finalizedByName}` : ''}
        </div>
      )}
      {showLines && artifact && (
        <div className="mt-3 overflow-x-auto">
          <LinesInline
            lines={artifact.lines ?? []}
            actorCanSeeLines={actorCanSeeLines}
            locale={localeSafe}
          />
        </div>
      )}
      <div className="mt-3 flex flex-wrap items-center gap-2">
        {artifact?.href && (
          <a
            href={artifact.href}
            className="rounded-lg border border-info bg-info px-3 py-1.5 text-sm font-mono text-info-soft hover:bg-info"
          >
            Open full →
          </a>
        )}
        {kind === 'po' && artifact?.href && (
          <a
            href={artifact.href}
            download
            className="rounded-lg border border-caution bg-caution px-3 py-1.5 text-sm font-mono text-caution-soft hover:bg-caution"
          >
            Download PO ↓
          </a>
        )}
        {approveAction && canApprove && (
          <form action={approveAction}>
            {approveHiddenInputs &&
              Object.entries(approveHiddenInputs).map(([k, v]) => (
                <input key={k} type="hidden" name={k} value={v} />
              ))}
            <input type="hidden" name="waybillId" value={_waybillId} />
            <button
              type="submit"
              className="rounded-lg bg-positive px-3 py-1.5 text-sm font-bold text-ink shadow shadow-positive hover:bg-positive-strong"
            >
              ✓ {approveLabel}
            </button>
          </form>
        )}
        {disabledReason && (
          <span className="rounded-full border border-rule bg-paper-2/60 px-2.5 py-1 text-xs font-mono text-ink-2">
            {disabledReason}
          </span>
        )}
      </div>
    </div>
  );
}
