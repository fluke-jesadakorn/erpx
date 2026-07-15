'use client';

import React, { useEffect, useRef, useState } from 'react';
import { T } from '@/components/i18n/T';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';

interface Props {
  waybillId: string;
  eventId: string;
  eventKind: string;
  fromStage: string;
  toStage: string;
  actorName?: string;
}

interface ExplainOk {
  ok: true;
  text: string;
  modelName: string;
  latencyMs: number;
}

interface ExplainFail {
  ok: false;
  error?: string;
}

export function EventExplainButton({
  waybillId,
  eventId,
  eventKind,
  fromStage,
  toStage,
  actorName,
}: Props) {
  const locale = useSecondaryLocale();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [meta, setMeta] = useState<{ modelName?: string; latencyMs?: number } | null>(null);
  const wrapRef = useRef<HTMLSpanElement | null>(null);

  useEffect(() => {
    if (!open) return;
    function onDocClick(e: MouseEvent) {
      if (!wrapRef.current) return;
      if (!wrapRef.current.contains(e.target as Node)) setOpen(false);
    }
    function onEsc(e: KeyboardEvent) {
      if (e.key === 'Escape') setOpen(false);
    }
    document.addEventListener('mousedown', onDocClick);
    document.addEventListener('keydown', onEsc);
    return () => {
      document.removeEventListener('mousedown', onDocClick);
      document.removeEventListener('keydown', onEsc);
    };
  }, [open]);

  async function run() {
    const next = !open;
    setOpen(next);
    if (!next) return;
    if (text || error) return;
    setBusy(true);
    setError(null);
    try {
      const res = await fetch('/api/waybill/explain-event', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          waybillId,
          eventId,
          eventKind,
          fromStage,
          toStage,
          actorName: actorName ?? null,
          lang: locale,
        }),
      });
      const data = (await res.json()) as ExplainOk | ExplainFail;
      if (data && (data as ExplainOk).ok) {
        const ok = data as ExplainOk;
        setText(ok.text || '');
        setMeta({ modelName: ok.modelName, latencyMs: ok.latencyMs });
      } else {
        setError((data as ExplainFail).error || 'AI unavailable');
      }
    } catch (e: any) {
      setError(e?.message || 'Network error');
    } finally {
      setBusy(false);
    }
  }

  function close() {
    setOpen(false);
  }

  return (
    <span ref={wrapRef} className="relative inline-block align-baseline">
      <button
        type="button"
        onClick={run}
        disabled={busy}
        aria-expanded={open}
        className="rounded bg-slate-800 px-1.5 py-0.5 text-xs text-slate-300 transition hover:bg-slate-700 disabled:opacity-60"
      >
        💬{' '}
        <T
          value={
            busy
              ? { en: 'asking…', th: 'กำลังถาม…', de: 'fragt…' }
              : { en: 'Why did this happen?', th: 'ทำไมถึงเกิดเหตุการณ์นี้?', de: 'Warum ist das passiert?' }
          }
        />
      </button>

      {open && (
        <div className="absolute left-0 top-full z-40 mt-1 w-full max-w-sm rounded-lg border border-slate-700/60 bg-slate-950/95 p-2.5 shadow-2xl shadow-slate-950/60 backdrop-blur">
          <div className="flex items-start justify-between gap-2">
            <div className="font-mono text-xs uppercase tracking-widest text-slate-400">
              <span aria-hidden>🤖</span>{' '}
              <span>
                {eventKind}
                {meta?.modelName ? ` · ${meta.modelName}` : ''}
                {meta?.latencyMs != null ? ` · ${meta.latencyMs}ms` : ''}
              </span>
            </div>
            <button
              type="button"
              onClick={close}
              aria-label={locale === 'th' ? 'ปิด' : locale === 'de' ? 'Schließen' : 'Close'}
              title="Close"
              className="grid h-4 w-4 place-items-center rounded bg-slate-800 font-mono text-xs text-slate-300 hover:bg-rose-900/60"
            >
              ✕
            </button>
          </div>

          {busy && (
            <p className="mt-1.5 font-mono text-xs text-slate-400">
              <T value={{ en: 'Asking the AI…', th: 'กำลังถาม AI…', de: 'Frage die KI…' }} />
            </p>
          )}

          {!busy && error && (
            <p className="mt-1.5 font-mono text-xs text-rose-300">✗ {error}</p>
          )}

          {!busy && !error && text && (
            <p className="mt-1.5 whitespace-pre-wrap font-sans text-sm leading-snug text-slate-100">
              {text}
            </p>
          )}

          {!busy && !error && !text && (
            <p className="mt-1.5 font-mono text-xs italic text-slate-500">
              <T value={{ en: 'No answer yet', th: 'ยังไม่มีคำตอบ', de: 'Noch keine Antwort' }} />
            </p>
          )}
        </div>
      )}
    </span>
  );
}

export default EventExplainButton;