import Link from 'next/link';
import type { ReportResult, ReportUnavailable } from '@/finance/reports';

export function FinancialReportView({ report }: { report: ReportResult | ReportUnavailable }) {
  if (!report.ok) {
    return <section className="panel-elevated p-6"><h2 className="text-lg font-bold text-critical">Report unavailable</h2><p className="mt-2 text-sm text-ink-2">{report.reason}</p></section>;
  }
  return <div className="space-y-5">
    <section className="panel-elevated p-5">
      <div className="flex flex-wrap items-start justify-between gap-3"><div><h2 className="text-xl font-black">{report.title}</h2><p className="text-sm text-ink-2">{report.subtitle}</p></div><div className="flex gap-2"><span className="glass-chip text-positive">Posted only</span><Link className="glass-chip" href={`/ledger?from=${report.period.date_from}&to=${report.period.date_to}`}>Drill to ledger</Link></div></div>
      <div className="mt-5 grid gap-3 sm:grid-cols-3">{report.kpis.map((kpi) => <div className="rounded-md border border-rule bg-paper-2 p-4" key={kpi.label}><div className="text-xs font-bold uppercase text-mute">{kpi.label}</div><div className={`mt-2 font-mono text-xl font-black ${kpi.tone === 'positive' ? 'text-positive' : kpi.tone === 'negative' ? 'text-critical' : 'text-ink'}`}>{kpi.value}</div>{kpi.hint && <div className="mt-1 text-xs text-ink-2">{kpi.hint}</div>}</div>)}</div>
    </section>
    {report.sections.map((section) => <section className="panel-elevated overflow-hidden" key={section.title}><div className="border-b border-rule px-5 py-4"><h3 className="text-lg font-bold">{section.title}</h3></div><div className="overflow-x-auto"><table className="w-full text-sm"><thead className="bg-paper-2 text-left text-xs font-bold uppercase text-mute"><tr>{section.columns.map((column) => <th className="px-4 py-3" key={column}>{column}</th>)}</tr></thead><tbody className="divide-y divide-rule">{section.rows.map((row, rowIndex) => <tr key={`${section.title}-${String(row[0])}-${rowIndex}`}>{row.map((cell, cellIndex) => <td className={`px-4 py-3 ${cellIndex >= 2 ? 'font-mono tabular-nums' : ''}`} key={`${cellIndex}-${String(cell)}`}>{cell}</td>)}</tr>)}{!section.rows.length && <tr><td className="px-4 py-8 text-center text-mute" colSpan={section.columns.length}>No posted activity.</td></tr>}</tbody>{section.total !== undefined && <tfoot className="border-t-2 border-rule bg-paper-2 font-bold"><tr><td className="px-4 py-3" colSpan={Math.max(1, section.columns.length - 1)}>{section.totalLabel ?? 'Total'}</td><td className="px-4 py-3 text-right font-mono">{Number(section.total).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}</td></tr></tfoot>}</table></div></section>)}
    {report.notes.length > 0 && <section className="rounded-md border border-caution bg-caution-soft p-4 text-sm"><ul className="list-disc space-y-1 pl-5">{report.notes.map((note) => <li key={note}>{note}</li>)}</ul></section>}
  </div>;
}
