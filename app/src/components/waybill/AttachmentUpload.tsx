'use client';

import React, { useEffect, useRef, useState } from 'react';
import {
  WAYBILL_KINDS,
  WAYBILL_KIND_ORDER,
  allowedKindsFor,
  type WaybillAttachmentKind,
} from '@folio-lib/waybill/kinds';
import { attachWaybillDocumentAction } from '@/app/actions';

interface Props {
  waybillId: string;
  stage: string;
}

interface PresignResponse {
  key: string;
  put_url: string;
  expires: number;
  kind: WaybillAttachmentKind;
  content_type: string;
  filename: string;
}

export function AttachmentUpload({ waybillId, stage }: Props) {
  const allowed = allowedKindsFor(stage);
  const [kind, setKind] = useState<WaybillAttachmentKind>(allowed[0] ?? 'other');
  const [busy, setBusy] = useState(false);
  const [phase, setPhase] = useState<'pick' | 'uploading' | 'recorded'>('pick');
  const [error, setError] = useState<string | null>(null);
  const [filename, setFilename] = useState<string>('');
  const [byteSize, setByteSize] = useState<number>(0);

  const storageKeyRef = useRef<string>('');
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setKind(allowed[0] ?? 'other');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stage]);

  function onPick(e: React.ChangeEvent<HTMLInputElement>): void {
    setError(null);
    const file = e.target.files?.[0];
    if (!file) {
      setFilename('');
      setByteSize(0);
      return;
    }
    setFilename(file.name);
    setByteSize(file.size);
  }

  async function onUpload(e: React.FormEvent<HTMLFormElement>): Promise<void> {
    e.preventDefault();
    setError(null);
    const file = fileRef.current?.files?.[0];
    if (!file) {
      setError('pick a file first');
      return;
    }
    if (file.size > 50 * 1024 * 1024) {
      setError('file too large (max 50 MB)');
      return;
    }
    setBusy(true);
    setPhase('uploading');
    try {
      const presignRes = await fetch(`/api/waybill/${waybillId}/attachments/presign`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ filename: file.name, contentType: file.type || 'application/octet-stream', kind }),
      });
      if (!presignRes.ok) {
        const text = await presignRes.text();
        throw new Error(`presign ${presignRes.status}: ${text}`);
      }
      const ps = (await presignRes.json()) as PresignResponse;
      storageKeyRef.current = ps.key;

      const putRes = await fetch(ps.put_url, {
        method: 'PUT',
        body: file,
        headers: { 'Content-Type': file.type || 'application/octet-stream' },
      });
      if (!putRes.ok) throw new Error(`upload failed: ${putRes.status}`);

      const form = new FormData();
      form.set('waybillId', waybillId);
      form.set('storageKey', ps.key);
      form.set('filename', file.name);
      form.set('contentType', file.type || 'application/octet-stream');
      form.set('byteSize', String(file.size));
      form.set('kind', kind);
      const captionEl = (e.currentTarget).querySelector<HTMLInputElement>('input[name="caption"]');
      if (captionEl?.value) form.set('caption', captionEl.value);

      setPhase('recorded');
      await attachWaybillDocumentAction(form);
    } catch (err) {
      setError((err as Error).message);
      setBusy(false);
      setPhase('pick');
    }
  }

  const usable = allowed.length > 0;

  return (
    <form
      onSubmit={(e) => void onUpload(e)}
      className="rounded-2xl border border-cyan-500/30 bg-cyan-950/15 p-3"
    >
      <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-cyan-300">
        <span>📤</span>
        <span>
          Attach document · stage: {stage}
          {busy ? ` · ${phase}` : ''}
        </span>
      </div>

      <div className="mt-3 grid gap-2 sm:grid-cols-2">
        <div className="space-y-1">
          <label className="text-xs font-mono text-slate-400">File (≤ 50 MB)</label>
          <input
            ref={fileRef}
            type="file"
            accept=".pdf,.jpg,.jpeg,.png,application/pdf,image/*"
            onChange={onPick}
            disabled={busy || !usable}
            className="block w-full rounded border border-slate-800 bg-slate-950 p-2 text-xs text-white file:mr-2 file:rounded file:border-0 file:bg-cyan-500/30 file:px-3 file:py-1 file:text-cyan-100"
          />
          {filename && (
            <div className="text-xs font-mono text-slate-500">
              {filename} · {byteSize ? `${(byteSize / 1024).toFixed(1)} KB` : ''}
            </div>
          )}
        </div>

        <div className="grid gap-2">
          <label className="text-xs font-mono text-slate-400">Kind</label>
          <select
            value={kind}
            onChange={(e) => setKind(e.target.value as WaybillAttachmentKind)}
            disabled={busy || !usable}
            className="rounded border border-slate-800 bg-slate-950 px-2 py-1.5 text-xs text-white"
          >
            {WAYBILL_KIND_ORDER.filter((k) => allowed.includes(k)).map((k) => {
              const meta = WAYBILL_KINDS[k];
              return (
                <option key={k} value={k}>
                  {meta.emoji} {k} — {meta.en}
                </option>
              );
            })}
          </select>
          <input
            type="text"
            name="caption"
            placeholder="Caption (optional)"
            disabled={busy}
            className="rounded border border-slate-800 bg-slate-950 px-2 py-1.5 text-xs text-white"
          />
        </div>
      </div>

      {error && (
        <div className="mt-2 rounded border border-rose-500/40 bg-rose-950/40 p-2 text-sm text-rose-200">
          ⚠ {error}
        </div>
      )}

      <div className="mt-3 flex items-center justify-between gap-2">
        <span className="text-xs font-mono text-slate-500">
          {allowed.length} kind{allowed.length === 1 ? '' : 's'} allowed at this stage
        </span>
        <button
          type="submit"
          disabled={busy || !usable || !filename}
          className="rounded-lg bg-cyan-500 px-3 py-1.5 text-xs font-bold text-slate-950 hover:bg-cyan-400 disabled:opacity-50"
        >
          {busy ? `Uploading (${phase})…` : '⤴ Upload & record event'}
        </button>
      </div>
    </form>
  );
}
