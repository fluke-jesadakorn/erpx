'use client';
import { useState } from 'react';

interface Row {
  [k: string]: unknown;
}

export function SqlResultTable({
  sql,
  columns,
  rows,
  rowCount,
  explanation,
}: {
  sql: string;
  columns: string[];
  rows: Row[];
  rowCount: number;
  explanation?: string;
}) {
  const [shown, setShown] = useState(Math.min(rows.length, 50));
  const csv = () => {
    const esc = (v: unknown) => {
      const s = v == null ? '' : String(v);
      return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
    };
    const lines = [
      columns.join(','),
      ...rows.slice(0, shown).map((r) => columns.map((c) => esc(r[c])).join(',')),
    ];
    navigator.clipboard?.writeText(lines.join('\n'));
  };
  return (
    <div className="folio-sql my-2 rounded-xl border border-cyan-700/40 bg-cyan-950/30 p-3">
      <div className="mb-2 flex items-center gap-2 text-xs font-mono text-cyan-300">
        <span className="rounded-full bg-cyan-500/20 px-2 py-0.5">SQL</span>
        <span className="text-slate-400">{rowCount} rows</span>
        <button
          type="button"
          onClick={csv}
          className="ml-auto rounded border border-cyan-700/40 px-2 py-0.5 text-cyan-200 hover:bg-cyan-500/10"
        >
          ⧉ csv
        </button>
      </div>
      <pre className="mb-2 overflow-x-auto rounded bg-slate-950/60 p-2 text-xs text-slate-300">
        <code>{sql}</code>
      </pre>
      <div className="max-h-80 overflow-auto rounded border border-slate-800">
        <table className="min-w-full text-xs">
          <thead className="sticky top-0 bg-slate-900 text-slate-300">
            <tr>
              {columns.map((c) => (
                <th key={c} className="px-2 py-1 text-left font-mono">
                  {c}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="font-mono text-slate-200">
            {rows.slice(0, shown).map((r, i) => (
              <tr key={i} className="odd:bg-slate-950 even:bg-slate-950/60">
                {columns.map((c) => (
                  <td key={c} className="px-2 py-1 align-top">
                    {r[c] == null ? '—' : String(r[c])}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {rows.length > shown && (
        <button
          type="button"
          onClick={() => setShown((s) => Math.min(rows.length, s + 100))}
          className="mt-2 text-xs text-cyan-300 hover:underline"
        >
          Show {Math.min(rows.length - shown, 100)} more
        </button>
      )}
      {explanation && (
        <div className="mt-2 text-xs text-slate-400">{explanation}</div>
      )}
    </div>
  );
}