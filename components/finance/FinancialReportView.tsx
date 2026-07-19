import Link from 'next/link';
import { ArrowRight, CheckCircle2, CircleAlert, FileSearch, ShieldCheck } from 'lucide-react';
import type { ReportResult, ReportUnavailable } from '@/finance/reports';
import { ReportAiAsk } from './ReportAiAsk';

export function FinancialReportView({ report, canUseAi }: { report: ReportResult | ReportUnavailable; canUseAi: boolean }) {
  if (!report.ok) {
    return <section className="panel-elevated p-6"><div className="flex gap-3"><CircleAlert className="mt-0.5 shrink-0 text-critical" size={20} /><div><h2 className="text-lg font-semibold text-critical">Report unavailable</h2><p className="mt-1 text-sm text-ink-2">{report.reason}</p></div></div></section>;
  }
  const base = {
    report: report.title,
    period: report.period,
    kpis: report.kpis,
    source_controls: report.source,
    notes: report.notes,
  };
  return <div className="space-y-5">
    <section className="panel-elevated relative overflow-hidden p-5 sm:p-6">
      <span aria-hidden className="absolute -right-20 -top-32 h-64 w-64 rounded-full bg-accent/10 blur-3xl" />
      <div className="relative flex flex-wrap items-start justify-between gap-5">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <span className="inline-flex items-center gap-1.5 rounded-full border border-positive/45 bg-positive-soft px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.12em] text-positive"><CheckCircle2 size={12} /> Posted only</span>
            <span className="rounded-full border border-rule bg-paper-2 px-2.5 py-1 font-mono text-[10px] text-mute">{report.period.date_from} → {report.period.date_to}</span>
          </div>
          <h2 className="mt-4 text-2xl font-semibold tracking-[-0.04em] text-ink sm:text-3xl">{report.title}</h2>
          <p className="mt-1 text-sm text-ink-2">{report.subtitle}</p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Link className="inline-flex h-9 items-center gap-2 rounded-full border border-rule bg-paper-2 px-3.5 text-xs font-semibold text-ink-2 transition hover:border-accent hover:text-ink" href={`/ledger?from=${report.period.date_from}&to=${report.period.date_to}`}><FileSearch size={14} /> Drill to ledger <ArrowRight size={13} /></Link>
          <ReportAiAsk title={`${report.title} analysis`} scope={`${report.title} headline metrics`} context={base} canUseAi={canUseAi} prompts={['Summarize this statement for the executive team.', 'What is the most material risk in these figures?', 'Which number should I drill into first and why?']} />
        </div>
      </div>
      <div className="relative mt-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">{report.kpis.map((kpi) => <div className="rounded-2xl border border-rule bg-paper/65 p-4 shadow-[inset_0_1px_0_var(--glass-highlight)]" key={kpi.label}><div className="text-[10px] font-semibold uppercase tracking-[0.14em] text-mute">{kpi.label}</div><div className={`mt-2 font-mono text-xl font-semibold tracking-[-0.035em] tabular-nums ${kpi.tone === 'positive' ? 'text-positive' : kpi.tone === 'negative' ? 'text-critical' : 'text-ink'}`}>{kpi.value}</div>{kpi.hint && <div className="mt-1 text-xs text-ink-2">{kpi.hint}</div>}</div>)}</div>
    </section>

    {report.sections.map((section, index) => <section className="panel-elevated overflow-hidden" key={section.title}>
      <div className="flex flex-wrap items-start justify-between gap-4 border-b border-rule px-5 py-4">
        <div><div className="text-[10px] font-semibold uppercase tracking-[0.14em] text-accent">Section {String(index + 1).padStart(2, '0')}</div><h3 className="mt-1 text-lg font-semibold tracking-[-0.025em] text-ink">{section.title}</h3><p className="mt-0.5 text-xs text-mute">{section.rows.length.toLocaleString()} posted row{section.rows.length === 1 ? '' : 's'}</p></div>
        <ReportAiAsk title={`${section.title} deep dive`} scope={`${report.title} · ${section.title}`} context={{ ...base, section }} canUseAi={canUseAi} prompts={[`Explain the movement in ${section.title}.`, 'Identify unusual or material account values.', 'Give me the next ledger drill-down steps.']} />
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="bg-paper-2/80 text-left text-[10px] font-semibold uppercase tracking-[0.12em] text-mute"><tr>{section.columns.map((column, columnIndex) => <th className={`whitespace-nowrap px-4 py-3 ${columnIndex >= 2 ? 'text-right' : ''}`} key={column}>{column}</th>)}</tr></thead>
          <tbody className="divide-y divide-rule">{section.rows.map((row, rowIndex) => <tr className="transition hover:bg-paper-2/65" key={`${section.title}-${String(row[0])}-${rowIndex}`}>{row.map((cell, cellIndex) => <td className={`whitespace-nowrap px-4 py-3 text-ink-2 ${cellIndex >= 2 ? 'text-right font-mono tabular-nums' : ''}`} key={`${cellIndex}-${String(cell)}`}>{cell}</td>)}</tr>)}{!section.rows.length && <tr><td className="px-4 py-12 text-center text-mute" colSpan={section.columns.length}>No posted activity in this scope.</td></tr>}</tbody>
          {section.total !== undefined && <tfoot className="border-t-2 border-rule bg-paper-2 font-semibold"><tr><td className="px-4 py-3 text-ink" colSpan={Math.max(1, section.columns.length - 1)}>{section.totalLabel ?? 'Total'}</td><td className="px-4 py-3 text-right font-mono text-ink tabular-nums">{Number(section.total).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}</td></tr></tfoot>}
        </table>
      </div>
    </section>)}

    <section className="grid gap-4 lg:grid-cols-2">
      <div className="rounded-2xl border border-positive/45 bg-positive-soft/60 p-5"><div className="flex gap-3"><ShieldCheck className="mt-0.5 shrink-0 text-positive" size={19} /><div><h3 className="text-sm font-semibold text-ink">Source controls</h3><p className="mt-1 text-sm leading-6 text-ink-2">Posted journals only. Drafts and operational commitments are excluded from statement values.</p><div className="mt-3 flex flex-wrap gap-2"><Source label="Posted only" ok={report.source.posted_only} /><Source label="Drafts excluded" ok={report.source.drafts_excluded} />{report.source.classification_complete !== undefined && <Source label="Classification complete" ok={report.source.classification_complete} />}{report.source.opening_balance_verified !== undefined && <Source label="Opening verified" ok={report.source.opening_balance_verified} />}</div></div></div></div>
      <div className={`rounded-2xl border p-5 ${report.notes.length ? 'border-caution/55 bg-caution-soft/60' : 'border-rule bg-paper/60'}`}><h3 className="text-sm font-semibold text-ink">Review notes</h3>{report.notes.length ? <ul className="mt-3 list-disc space-y-1.5 pl-5 text-sm leading-6 text-ink-2">{report.notes.map((note) => <li key={note}>{note}</li>)}</ul> : <p className="mt-2 text-sm text-ink-2">No report exceptions were raised for this scope.</p>}</div>
    </section>
  </div>;
}

function Source({ label, ok }: { label: string; ok: boolean }) {
  return <span className={`rounded-full border px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wider ${ok ? 'border-positive/45 bg-paper/35 text-positive' : 'border-caution/45 bg-paper/35 text-caution'}`}>{label}</span>;
}
