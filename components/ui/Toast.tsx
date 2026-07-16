'use client';

import React, { createContext, useCallback, useContext, useEffect, useState } from 'react';
import { T } from '@/components/i18n/T';

export type ToastKind = 'success' | 'error' | 'info' | 'warning';

export interface Toast {
  id: number;
  kind: ToastKind;
  title?: string;
  message: string;
  durationMs: number;
}

interface ToastContextValue {
  toasts: Toast[];
  push: (t: Omit<Toast, 'id' | 'durationMs'> & { durationMs?: number }) => number;
  dismiss: (id: number) => void;
}

const ToastCtx = createContext<ToastContextValue | null>(null);

const TONE: Record<ToastKind, { bar: string; glyph: string; ring: string }> = {
  success: { bar: 'bg-emerald-500', glyph: '✓', ring: 'border-emerald-500/30' },
  error:   { bar: 'bg-rose-500',    glyph: '✗', ring: 'border-rose-500/40'    },
  warning: { bar: 'bg-amber-500',   glyph: '⚠', ring: 'border-amber-500/40'   },
  info:    { bar: 'bg-indigo-500',  glyph: 'ℹ', ring: 'border-indigo-500/40'  },
};

export const ToastProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const dismiss = useCallback((id: number) => {
    setToasts((arr) => arr.filter((t) => t.id !== id));
  }, []);

  const push = useCallback<ToastContextValue['push']>((t) => {
    const id = Date.now() + Math.floor(Math.random() * 1000);
    const durationMs = t.durationMs ?? 4500;
    setToasts((arr) => [...arr, { id, kind: t.kind, title: t.title, message: t.message, durationMs }]);
    return id;
  }, []);

  return (
    <ToastCtx.Provider value={{ toasts, push, dismiss }}>
      {children}
      <ToastViewport toasts={toasts} dismiss={dismiss} />
    </ToastCtx.Provider>
  );
};

export function useToast() {
  const ctx = useContext(ToastCtx);
  if (!ctx) {
    // Fallback to a no-op if used outside the provider (e.g. in unit tests)
    return {
      toasts: [] as Toast[],
      push: () => 0,
      dismiss: () => {},
      success: (_message: string, _title?: string) => 0,
      error:   (_message: string, _title?: string) => 0,
      info:    (_message: string, _title?: string) => 0,
      warning: (_message: string, _title?: string) => 0,
    };
  }
  return {
    ...ctx,
    success: (message: string, title?: string) => ctx.push({ kind: 'success', message, title }),
    error:   (message: string, title?: string) => ctx.push({ kind: 'error',   message, title }),
    info:    (message: string, title?: string) => ctx.push({ kind: 'info',    message, title }),
    warning: (message: string, title?: string) => ctx.push({ kind: 'warning', message, title }),
  };
}

const ToastViewport: React.FC<{ toasts: Toast[]; dismiss: (id: number) => void }> = ({ toasts, dismiss }) => {
  return (
    <div
      role="region"
      aria-label="Notifications"
      className="fixed bottom-4 right-4 z-[200] flex flex-col gap-2 max-w-sm w-[calc(100vw-2rem)] sm:w-96 pointer-events-none"
    >
      {toasts.map((t) => (
        <ToastCard key={t.id} toast={t} onDismiss={() => dismiss(t.id)} />
      ))}
    </div>
  );
};

const ToastCard: React.FC<{ toast: Toast; onDismiss: () => void }> = ({ toast, onDismiss }) => {
  const tone = TONE[toast.kind];
  useEffect(() => {
    const tm = setTimeout(onDismiss, toast.durationMs);
    return () => clearTimeout(tm);
  }, [toast.id, toast.durationMs, onDismiss]);
  return (
    <div
      role={toast.kind === 'error' ? 'alert' : 'status'}
      className={`pointer-events-auto glass-panel-heavy rounded-2xl border ${tone.ring} shadow-2xl shadow-black/40 overflow-hidden animate-fade-in`}
    >
      <div className="flex items-stretch">
        <div className={`w-1 ${tone.bar}`} aria-hidden />
        <div className="flex-1 p-3 min-w-0">
          <div className="flex items-start gap-2">
            <span className="text-base leading-none mt-0.5">{tone.glyph}</span>
            <div className="flex-1 min-w-0">
              {toast.title && (
                <div className="text-sm font-mono font-bold uppercase tracking-wider text-slate-200">
                  {toast.title}
                </div>
              )}
              <div className="text-xs text-slate-100 whitespace-pre-wrap font-sans leading-relaxed break-words">
                {toast.message}
              </div>
            </div>
            <button
              type="button"
              onClick={onDismiss}
              aria-label="Dismiss"
              className="text-slate-500 hover:text-white w-5 h-5 inline-flex items-center justify-center text-xs shrink-0"
            >
              <span aria-hidden>✕</span>
              <span className="sr-only"><T id="common.close" hideSecondary /></span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
