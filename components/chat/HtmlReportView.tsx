'use client';

import { useMemo, useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { Badge, Panel, Tabs } from '@/components/ui';
import { buildHtmlReport } from './htmlReport';

interface Row {
  [key: string]: unknown;
}

export function HtmlReportView({ title, columns, rows, rowCount, sql, explanation, lang = 'en' }: { title?: string; columns: string[]; rows: Row[]; rowCount: number; sql?: string; explanation?: string; lang?: 'en' | 'th' | 'de' }) {
  const [tab, setTab] = useState<'report' | 'sql'>('report');
  const [copied, setCopied] = useState<string | null>(null);
  const report = useMemo(() => buildHtmlReport(columns, rows, { title: title ?? 'Query result', subtitle: `${rowCount} ${rowCount === 1 ? 'row' : 'rows'}`, lang }), [columns, rows, rowCount, title, lang]);

  const copy = (key: 'html' | 'css' | 'js' | 'full') => {
    const text = key === 'html' ? report.html : key === 'css' ? report.css : key === 'js' ? report.js : report.full;
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(key);
      setTimeout(() => setCopied((current) => current === key ? null : current), 1200);
    });
  };

  return (
    <Panel padding="none" className="folio-html-report my-2 overflow-hidden border-accent/40">
      <header className="flex flex-wrap items-center gap-2 border-b border-rule bg-paper-2 px-3 py-2">
        <Badge tone="accent">HTML</Badge>
        <Badge tone="positive"><Check size={12} />source: ask_sql</Badge>
        <span className="text-xs text-mute">{rowCount} {rowCount === 1 ? 'row' : 'rows'}</span>
        <Tabs value={tab} onValueChange={(value) => setTab(value as 'report' | 'sql')} items={[{ value: 'report', label: 'preview' }, { value: 'sql', label: 'sql' }]} className="ml-2" />
        <div className="ml-auto flex flex-wrap gap-1">
          {(['html', 'css', 'js', 'full'] as const).map((key) => <button key={key} type="button" onClick={() => copy(key)} className="inline-flex h-7 items-center gap-1 rounded-md border border-rule bg-paper px-2 font-mono text-xs text-ink-2 hover:bg-paper-3">{copied === key ? <Check size={12} /> : <Copy size={12} />}{key}</button>)}
        </div>
      </header>
      {tab === 'report' ? <iframe title={title ?? 'Query result'} srcDoc={report.full} sandbox="allow-scripts allow-downloads" allow="clipboard-write" className="block h-[420px] w-full border-0 bg-paper" /> : <pre className="max-h-80 overflow-auto bg-paper p-3 font-mono text-xs text-ink-2"><code>{sql}</code></pre>}
      {explanation && <div className="border-t border-rule bg-paper-2 px-3 py-2 text-xs text-mute">{explanation}</div>}
    </Panel>
  );
}
