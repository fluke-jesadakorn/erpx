'use client';

import Link from 'next/link';
import { useMemo, useState } from 'react';
import { Modal } from '@/components/ui/Modal';
import { useToast } from '@/components/ui/Toast';

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
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const toast = useToast();
  const statuses = useMemo(
    () => [...new Set(rows.map((row) => row.status))].sort(),
    [rows],
  );
  const visible = status === 'all' ? rows : rows.filter((row) => row.status === status);

  async function remove(id: string) {
    setBusy(id);
    setError(null);
    try {
      const res = await fetch(`/api/law/contracts?id=${encodeURIComponent(id)}`, {
        method: 'DELETE',
      });
      const body = await res.json();
      if (!res.ok) throw new Error(body.error || 'Delete failed');
      setRows((current) => current.filter((row) => row.id !== id));
      toast.success('Contract deleted.');
    } catch (err) {
      const msg = (err as Error).message;
      setError(msg);
      toast.error(msg);
    } finally {
      setBusy(null);
      setDeletingId(null);
    }
  }

  return (
    <section className="rounded-md border border-rule bg-paper-2/55">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-rule px-4 py-3">
        <div>
          <h2 className="text-sm font-semibold text-ink">Contracts</h2>
          <p className="mt-0.5 text-xs text-mute">{visible.length} documents</p>
        </div>
        <select
          value={status}
          onChange={(event) => setStatus(event.target.value)}
          className="rounded-lg border border-rule bg-paper px-3 py-2 text-xs text-ink outline-none focus:border-info/40"
          aria-label="Filter by status"
        >
          <option value="all">All statuses</option>
          {statuses.map((value) => (
            <option key={value} value={value}>{value}</option>
          ))}
        </select>
      </div>

      {error && (
        <p className="border-b border-critical/40 bg-critical px-4 py-2 text-xs text-critical">{error}</p>
      )}

      <div className="overflow-x-auto">
        <table className="w-full min-w-[760px] text-left text-sm">
          <thead className="bg-paper-2/70 text-xs uppercase tracking-wider text-mute">
            <tr>
              <th className="px-4 py-3">Document</th>
              <th className="px-4 py-3">Category</th>
              <th className="px-4 py-3">Status</th>
              <th className="px-4 py-3 text-right">Chunks</th>
              <th className="px-4 py-3">Uploaded</th>
              <th className="px-4 py-3 text-right">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-rule">
            {visible.map((row) => (
              <tr key={row.id} className="text-ink-2 hover:bg-paper-2/45">
                <td className="px-4 py-3">
                  <Link href={`/law/${row.id}`} className="font-medium text-info hover:text-info">
                    {row.fileName}
                  </Link>
                  <div className="mt-1 font-mono text-xs text-mute">
                    {row.docNo || row.id.slice(0, 8)}
                    {row.sizeBytes != null ? ` · ${(row.sizeBytes / 1024).toFixed(1)} KB` : ''}
                  </div>
                </td>
                <td className="px-4 py-3 text-ink-2">{row.category || 'Uncategorized'}</td>
                <td className="px-4 py-3">
                  <span className="rounded-full border border-rule bg-paper px-2 py-1 font-mono text-xs text-ink-2">
                    {row.status}
                  </span>
                </td>
                <td className="px-4 py-3 text-right font-mono text-ink-2">{row.chunkCount}</td>
                <td className="px-4 py-3 text-xs text-mute">
                  {new Date(row.uploadedAt).toLocaleString()}
                </td>
                <td className="px-4 py-3 text-right">
                  <div className="flex justify-end gap-2">
                    <Link
                      href={`/law/${row.id}`}
                      className="rounded-lg border border-info/40 px-2.5 py-1.5 text-xs text-info hover:bg-info"
                    >
                      Open
                    </Link>
                    <button
                      type="button"
                      onClick={() => setDeletingId(row.id)}
                      disabled={busy === row.id}
                      className="rounded-lg border border-critical/40 px-2.5 py-1.5 text-xs text-critical hover:bg-critical disabled:opacity-50"
                    >
                      Delete
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {visible.length === 0 && (
        <div className="px-6 py-12 text-center text-sm text-mute">No contracts match this filter.</div>
      )}

      <Modal
        open={deletingId !== null}
        onClose={() => (busy ? null : setDeletingId(null))}
        title="Delete contract"
        subtitle="This will remove the document and its indexed content."
        tone="rose"
        width="md"
        footer={
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setDeletingId(null)}
              disabled={busy !== null}
              className="rounded-lg border border-rule px-3 py-1.5 text-xs text-ink-2 hover:bg-paper-2 disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => deletingId && remove(deletingId)}
              disabled={busy !== null}
              className="rounded-lg bg-critical px-3 py-1.5 text-xs text-paper hover:bg-critical-strong disabled:opacity-50"
            >
              {busy ? 'Deleting…' : 'Delete'}
            </button>
          </div>
        }
      >
        <p className="text-sm text-ink-2">
          Delete <span className="font-mono text-ink">{deletingId?.slice(0, 8)}</span>? This cannot be undone.
        </p>
      </Modal>
    </section>
  );
}
