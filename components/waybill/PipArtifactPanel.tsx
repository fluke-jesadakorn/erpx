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
  pr:             { emoji: '📄', tone: 'border-cyan-500/40 bg-cyan-950/20' },
  po:             { emoji: '📦', tone: 'border-amber-500/40 bg-amber-950/20' },
  'gl-accrual':   { emoji: '📒', tone: 'border-cyan-500/40 bg-cyan-950/15' },
  'gl-settlement':{ emoji: '📒', tone: 'border-amber-500/40 bg-amber-950/15' },
  paySlip:        { emoji: '💳', tone: 'border-emerald-500/40 bg-emerald-950/20' },
};

function StatusPill({ status }: { status: string | null }) {
  const map: Record<string, string> = {
    draft: 'bg-slate-700 text-slate-200 border-slate-600',
    submission: 'bg-cyan-500/15 text-cyan-200 border-cyan-400/50',
    accounting_authorization: 'bg-amber-500/15 text-amber-200 border-amber-400/50',
    cfo_authorization: 'bg-amber-500/15 text-amber-200 border-amber-400/50',
    awaiting_disbursement: 'bg-indigo-500/15 text-indigo-200 border-indigo-400/50',
    finalized: 'bg-emerald-500/15 text-emerald-200 border-emerald-400/50',
    issued: 'bg-emerald-500/15 text-emerald-200 border-emerald-400/50',
    settled: 'bg-emerald-500/15 text-emerald-200 border-emerald-400/50',
    confirmed: 'bg-emerald-500/15 text-emerald-200 border-emerald-400/50',
    approved: 'bg-emerald-500/15 text-emerald-200 border-emerald-400/50',
    rejected: 'bg-rose-500/15 text-rose-200 border-rose-400/50',
  };
  const cls = map[status ?? ''] ?? 'bg-slate-700 text-slate-200 border-slate-600';
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
          <tr className="text-left font-mono text-xs uppercase tracking-widest text-slate-500">
            <th className="border-b border-slate-800/60 px-2 py-1">
              <T id="waybill.gl.code" locale={locale} />
            </th>
            <th className="border-b border-slate-800/60 px-2 py-1">
              <T id="waybill.gl.account" locale={locale} />
            </th>
            <th className="border-b border-slate-800/60 px-2 py-1 text-right">
              <T id="waybill.gl.debit" locale={locale} />
            </th>
            <th className="border-b border-slate-800/60 px-2 py-1 text-right">
              <T id="waybill.gl.credit" locale={locale} />
            </th>
          </tr>
        </thead>
        <tbody>
          {lines.map((l, i) => (
            <tr key={i} className="align-top font-mono text-slate-300">
              <td className="border-b border-slate-800/40 px-2 py-1 text-cyan-300">{l.account_code}</td>
              <td className="border-b border-slate-800/40 px-2 py-1 text-slate-200">
                {locale === 'th' ? l.account_name_th ?? l.account_name ?? '—' : l.account_name ?? l.account_name_th ?? '—'}
              </td>
              <td className="border-b border-slate-800/40 px-2 py-1 text-right text-emerald-200 tabular-nums">
                {l.debit > 0 ? fmtNum(l.debit) : ''}
              </td>
              <td className="border-b border-slate-800/40 px-2 py-1 text-right text-amber-200 tabular-nums">
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
    <div className={`rounded-2xl border p-4 ${meta.tone}`}>
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="flex items-center gap-2">
          <span aria-hidden className="text-lg leading-none">{meta.emoji}</span>
          <span className="text-sm font-bold text-white">
            <T id={`waybill.pip.${kind === 'paySlip' ? 'paymentSlip' : kind === 'pr' ? 'openPr' : kind === 'po' ? 'openPo' : kind === 'gl-accrual' ? 'glBeforePay' : 'glAfterPay'}`} locale={localeSafe} />
          </span>
        </div>
        <StatusPill status={artifact?.status ?? null} />
      </div>
      <div className="mt-2 font-mono text-base text-cyan-200">
        {artifact?.id ?? '—'}
      </div>
      {artifact?.finalizedAt && (
        <div className="mt-1 text-xs font-mono text-slate-500">
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
            className="rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-1.5 text-sm font-mono text-cyan-200 hover:bg-cyan-500/30"
          >
            Open full →
          </a>
        )}
        {kind === 'po' && artifact?.href && (
          <a
            href={artifact.href}
            download
            className="rounded-lg border border-amber-500/40 bg-amber-500/10 px-3 py-1.5 text-sm font-mono text-amber-200 hover:bg-amber-500/30"
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
              className="rounded-lg bg-gradient-to-br from-emerald-400 to-cyan-500 px-3 py-1.5 text-sm font-bold text-slate-950 shadow shadow-emerald-500/30 hover:from-emerald-300"
            >
              ✓ {approveLabel}
            </button>
          </form>
        )}
        {disabledReason && (
          <span className="rounded-full border border-slate-700 bg-slate-900/60 px-2.5 py-1 text-xs font-mono text-slate-400">
            {disabledReason}
          </span>
        )}
      </div>
    </div>
  );
}
