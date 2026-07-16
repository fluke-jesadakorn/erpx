'use client';

import React, { useEffect, useRef, useState } from 'react';
import {
  WAYBILL_KINDS,
  WAYBILL_KIND_ORDER,
  allowedKindsFor,
  type WaybillAttachmentKind,
} from '@/waybill/kinds';
import { attachWaybillDocumentAction } from '@/app/actions/waybill';
import { T } from '@/components/i18n/T';

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
      className="rounded-2xl border-2 border-info/30 bg-info-soft/40 p-4 space-y-3"
    >
      <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-info-strong font-bold">
        <span>📤</span>
        <span>
           <T id="waybill.attachment.attachTitle" /> · <T id="waybill.attachment.stagePrefix" />: {stage}
          {busy ? ` · ${phase}` : ''}
        </span>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1.5">
          <label className="text-xs font-mono uppercase tracking-wider text-ink font-bold">           <T id="waybill.attachment.fileLimit" /></label>
          <input
            ref={fileRef}
            type="file"
            accept=".pdf,.jpg,.jpeg,.png,application/pdf,image/*"
            onChange={onPick}
            disabled={busy || !usable}
            className="block w-full rounded-lg border border-rule bg-paper-2 p-2 text-xs text-ink file:mr-3 file:rounded-md file:border-0 file:bg-accent file:px-3 file:py-1.5 file:text-paper-2 file:font-bold file:cursor-pointer"
          />
          {filename && (
            <div className="text-xs font-mono text-ink-2">
              {filename} · {byteSize ? `${(byteSize / 1024).toFixed(1)} KB` : ''}
            </div>
          )}
        </div>

        <div className="space-y-3">
          <div className="space-y-1.5">
             <label className="text-xs font-mono uppercase tracking-wider text-ink font-bold"><T id="waybill.attachment.kindSelect" /></label>
            <select
              value={kind}
              onChange={(e) => setKind(e.target.value as WaybillAttachmentKind)}
              disabled={busy || !usable}
              className="w-full rounded-lg border border-rule bg-paper-2 px-3 py-2 text-xs text-ink focus:outline-none focus:border-accent focus:ring-2 focus:ring-accent/25"
            >
              {WAYBILL_KIND_ORDER.filter((k) => allowed.includes(k)).map((k) => {
                const meta = WAYBILL_KINDS[k];
                return (
                  <option key={k} value={k}>
                     {meta.emoji} {k} — <T id={meta.id} />
                  </option>
                );
              })}
            </select>
          </div>
          <input
            type="text"
            name="caption"
            placeholder="Caption (optional)"
            disabled={busy}
            className="w-full rounded-lg border border-rule bg-paper-2 px-3 py-2 text-xs text-ink placeholder:text-mute/60 focus:outline-none focus:border-accent focus:ring-2 focus:ring-accent/25"
          />
        </div>
      </div>

      {error && (
        <div className="rounded-lg border border-critical/40 bg-critical-soft p-2.5 text-sm text-critical">
          ⚠ {error}
        </div>
      )}

      <div className="flex items-center justify-between gap-2 pt-1">
        <span className="text-xs font-mono text-ink-2">
           <T id="waybill.attachment.kindsAllowed" values={{ n: allowed.length }} />
        </span>
        <button
          type="submit"
          disabled={busy || !usable || !filename}
          className="rounded-lg bg-accent px-4 py-2 text-xs font-bold text-paper-2 hover:bg-accent-strong disabled:opacity-50 transition-colors"
        >
           {busy ? <T id="waybill.attachment.busy" values={{ phase }} /> : <T id="waybill.attachment.attachRecordEvent" />}
        </button>
      </div>
    </form>
  );
}
