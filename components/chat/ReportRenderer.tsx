'use client';

import type { ReportResult, ReportUnavailable } from '@folio-lib/finance/reports';
import { Alert, Badge, Kpi, Panel, Status } from '@/components/ui';

function SectionTable({ section, lang }: { section: ReportResult['sections'][number]; lang: string }) {
  const cols = section.columns.length;
  return (
    <section className="mt-4">
      <h4 className="border-l-2 border-accent bg-accent-soft px-3 py-2 text-sm font-semibold text-ink">{lang === 'th' ? section.titleTh : section.title}</h4>
      <div className="overflow-x-auto rounded-md border border-rule">
        <table className="w-full text-sm">
          <thead className="bg-paper-3 text-xs uppercase tracking-wider text-mute"><tr>{section.columns.map((col, i) => <th key={i} className={['px-3 py-2 font-medium', i >= cols - 1 ? 'text-right' : 'text-left'].join(' ')}>{col}</th>)}</tr></thead>
          <tbody>
            {section.rows.length === 0 ? <tr><td colSpan={cols} className="px-3 py-4 text-center text-xs text-mute">{lang === 'th' ? 'ไม่มีรายการ' : 'No entries'}</td></tr> : section.rows.map((row, ri) => <tr key={ri} className="border-t border-rule hover:bg-paper-3">{row.map((cell, ci) => <td key={ci} className={['px-3 py-2 text-ink-2', ci >= cols - 1 ? 'text-right tabular-nums' : 'text-left'].join(' ')}>{cell}</td>)}</tr>)}
            {section.total != null && <tr className="border-t border-rule bg-accent-soft"><td colSpan={cols - 1} className="px-3 py-2 text-xs font-semibold text-accent">{section.totalLabel ?? 'Total'}</td><td className="px-3 py-2 text-right font-mono font-semibold tabular-nums text-accent">{Number(section.total).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}</td></tr>}
          </tbody>
        </table>
      </div>
    </section>
  );
}

export function ReportRenderer({ report }: { report: ReportResult }) {
  const th = report.lang === 'th';
  return (
    <Panel tone="elevated" className="folio-report my-2">
      <header className="flex flex-wrap items-end justify-between gap-3 border-b border-rule pb-4">
        <div>
          <div className="text-xs font-semibold uppercase tracking-wider text-accent">{th ? 'รายงาน' : 'Report'} · {report.intent.replace(/_/g, ' ')}</div>
          <h3 className="mt-1 text-lg font-semibold text-ink">{th ? report.titleTh : report.title}</h3>
          <div className="mt-1 text-sm text-ink-2">{th ? report.subtitleTh : report.subtitle}</div>
        </div>
        <div className="flex flex-wrap gap-1"><Badge tone="positive">posted-only</Badge><Badge tone="accent">drafts excluded</Badge><Badge tone={report.source.classification_complete ? 'positive' : 'caution'}>classification {report.source.classification_complete ? 'verified' : 'pending'}</Badge><Badge tone={report.source.opening_balance_verified ? 'positive' : 'caution'}>opening {report.source.opening_balance_verified ? 'verified' : 'pending'}</Badge></div>
      </header>
      <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        {report.kpis.map((kpi, i) => <Kpi key={i} title={th ? kpi.labelTh : kpi.label} value={<Status dot={false} tone={kpi.tone === 'positive' ? 'positive' : kpi.tone === 'negative' ? 'critical' : 'neutral'} className="text-2xl tabular-nums">{kpi.value}</Status>} delta={kpi.hint ? { value: kpi.hint, tone: 'neutral' } : undefined} />)}
      </div>
      {report.sections.map((section, i) => <SectionTable key={i} section={section} lang={report.lang} />)}
      {report.notes.length > 0 && <Alert tone="caution" title="Report notes" className="mt-4"><div className="space-y-1">{report.notes.map((note, i) => <div key={i}>{note}</div>)}</div></Alert>}
    </Panel>
  );
}

export function ReportUnavailableView({ report }: { report: ReportUnavailable }) {
  return <Alert tone="critical" title={report.lang === 'th' ? 'ไม่สามารถสร้างรายงาน' : 'Cannot generate report'} className="my-2">{report.reason}</Alert>;
}
