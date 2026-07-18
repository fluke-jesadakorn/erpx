'use client';

import React, { createContext, useCallback, useContext, useEffect, useState } from 'react';
import { CheckCircle, CircleAlert, Info, TriangleAlert, X, type LucideIcon } from 'lucide-react';
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

const TONE: Record<ToastKind, { bar: string; icon: LucideIcon; ring: string; text: string }> = {
  success: { bar: 'bg-positive', icon: CheckCircle, ring: 'border-positive/40', text: 'text-positive' },
  error:   { bar: 'bg-critical', icon: CircleAlert, ring: 'border-critical/40', text: 'text-critical' },
  warning: { bar: 'bg-caution', icon: TriangleAlert, ring: 'border-caution/40', text: 'text-caution' },
  info:    { bar: 'bg-info', icon: Info, ring: 'border-info/40', text: 'text-info' },
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
      className="fixed bottom-4 right-4 z-toast flex flex-col gap-2 max-w-sm w-[calc(100vw-2rem)] sm:w-96 pointer-events-none"
    >
      {toasts.map((t) => (
        <ToastCard key={t.id} toast={t} onDismiss={() => dismiss(t.id)} />
      ))}
    </div>
  );
};

const ToastCard: React.FC<{ toast: Toast; onDismiss: () => void }> = ({ toast, onDismiss }) => {
  const tone = TONE[toast.kind];
  const ToneIcon = tone.icon;
  useEffect(() => {
    const tm = setTimeout(onDismiss, toast.durationMs);
    return () => clearTimeout(tm);
  }, [toast.id, toast.durationMs, onDismiss]);
  return (
    <div
      role={toast.kind === 'error' ? 'alert' : 'status'}
      className={`panel-floating pointer-events-auto overflow-hidden ${tone.ring} animate-fade-scale`}
    >
      <div className="flex items-stretch">
        <div className={`w-1 ${tone.bar}`} aria-hidden />
        <div className="flex-1 p-3 min-w-0">
          <div className="flex items-start gap-2">
            <ToneIcon size={17} className={`mt-0.5 shrink-0 ${tone.text}`} aria-hidden />
            <div className="flex-1 min-w-0">
              {toast.title && (
                <div className="text-sm font-mono font-bold uppercase tracking-wider text-ink">
                  {toast.title}
                </div>
              )}
              <div className="text-xs text-ink whitespace-pre-wrap font-sans leading-relaxed break-words">
                {toast.message}
              </div>
            </div>
            <button
              type="button"
              onClick={onDismiss}
              aria-label="Dismiss"
              className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-mute transition-colors hover:bg-paper-3/50 hover:text-ink"
            >
              <X size={13} aria-hidden />
              <span className="sr-only"><T id="common.close" hideSecondary /></span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
