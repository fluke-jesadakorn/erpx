'use client';

import Link from 'next/link';
import { useMemo, useState } from 'react';

export interface LawContractRow {
  id: string;
  fileName: string;
  fileType: string | null;
  category: string | null;
  status: string;
  chunkCount: number;
  sizeBytes: number | null;
  docNo: string | null;
  source: string | null;
  uploadedAt: string;
}

export function ContractList({ contracts }: { contracts: LawContractRow[] }) {
  const [rows, setRows] = useState(contracts);
  const [status, setStatus] = useState('all');
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const statuses = useMemo(
    () => [...new Set(rows.map((row) => row.status))].sort(),
    [rows],
  );
  const visible = status === 'all' ? rows : rows.filter((row) => row.status === status);

  async function remove(id: string) {
    if (!window.confirm('Delete this contract and its indexed content?')) return;
    setBusy(id);
    setError(null);
    try {
      const res = await fetch(`/api/law/contracts?id=${encodeURIComponent(id)}`, {
        method: 'DELETE',
      });
      const body = await res.json();
      if (!res.ok) throw new Error(body.error || 'Delete failed');
      setRows((current) => current.filter((row) => row.id !== id));
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(null);
    }
  }

  return (
    <section className="rounded-2xl border border-slate-800 bg-slate-950/55">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-slate-800 px-4 py-3">
        <div>
          <h2 className="text-sm font-semibold text-slate-100">Contracts</h2>
          <p className="mt-0.5 text-xs text-slate-500">{visible.length} documents</p>
        </div>
        <select
          value={status}
          onChange={(event) => setStatus(event.target.value)}
          className="rounded-lg border border-slate-700 bg-slate-900 px-3 py-2 text-xs text-slate-200 outline-none focus:border-cyan-500"
          aria-label="Filter by status"
        >
          <option value="all">All statuses</option>
          {statuses.map((value) => (
            <option key={value} value={value}>{value}</option>
          ))}
        </select>
      </div>

      {error && (
        <p className="border-b border-rose-500/30 bg-rose-500/10 px-4 py-2 text-xs text-rose-200">{error}</p>
      )}

      <div className="overflow-x-auto">
        <table className="w-full min-w-[760px] text-left text-sm">
          <thead className="bg-slate-900/70 text-xs uppercase tracking-wider text-slate-500">
            <tr>
              <th className="px-4 py-3">Document</th>
              <th className="px-4 py-3">Category</th>
              <th className="px-4 py-3">Status</th>
              <th className="px-4 py-3 text-right">Chunks</th>
              <th className="px-4 py-3">Uploaded</th>
              <th className="px-4 py-3 text-right">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800/80">
            {visible.map((row) => (
              <tr key={row.id} className="text-slate-300 hover:bg-slate-900/45">
                <td className="px-4 py-3">
                  <Link href={`/law/${row.id}`} className="font-medium text-cyan-300 hover:text-cyan-200">
                    {row.fileName}
                  </Link>
                  <div className="mt-1 font-mono text-xs text-slate-500">
                    {row.docNo || row.id.slice(0, 8)}
                    {row.sizeBytes != null ? ` · ${(row.sizeBytes / 1024).toFixed(1)} KB` : ''}
                  </div>
                </td>
                <td className="px-4 py-3 text-slate-400">{row.category || 'Uncategorized'}</td>
                <td className="px-4 py-3">
                  <span className="rounded-full border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs text-slate-300">
                    {row.status}
                  </span>
                </td>
                <td className="px-4 py-3 text-right font-mono text-slate-400">{row.chunkCount}</td>
                <td className="px-4 py-3 text-xs text-slate-500">
                  {new Date(row.uploadedAt).toLocaleString()}
                </td>
                <td className="px-4 py-3 text-right">
                  <div className="flex justify-end gap-2">
                    <Link
                      href={`/law/${row.id}`}
                      className="rounded-lg border border-cyan-500/40 px-2.5 py-1.5 text-xs text-cyan-200 hover:bg-cyan-500/10"
                    >
                      Open
                    </Link>
                    <button
                      type="button"
                      onClick={() => remove(row.id)}
                      disabled={busy === row.id}
                      className="rounded-lg border border-rose-500/40 px-2.5 py-1.5 text-xs text-rose-200 hover:bg-rose-500/10 disabled:opacity-50"
                    >
                      {busy === row.id ? 'Deleting' : 'Delete'}
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {visible.length === 0 && (
        <div className="px-6 py-12 text-center text-sm text-slate-500">No contracts match this filter.</div>
      )}
    </section>
  );
}
