'use client';

import { useMemo, useState, type ReactNode } from 'react';
import { Bilingual } from '@/components/i18n/Bilingual';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';

export interface NewWaybillPanelProps {
  domain: 'expense' | 'sales';
  currentUserId: number;
  initialDraft?: unknown;
  title: string;
  titleTh: string;
  discardLabel: ReactNode;
  submitLabel: ReactNode;
  readyToSubmit: boolean;
  submitting: boolean;
  onSubmit: () => void;
  onDiscard: () => void;
  children: ReactNode;
  hint: ReactNode;
  draftWaybillId: string | null;
}

export function NewWaybillPanel({
  domain,
  currentUserId: _currentUserId,
  initialDraft: _initialDraft,
  title: _title,
  titleTh: _titleTh,
  discardLabel,
  submitLabel,
  readyToSubmit,
  submitting,
  onSubmit,
  onDiscard,
  children,
  hint,
  draftWaybillId,
}: NewWaybillPanelProps) {
  const locale = useSecondaryLocale();
  const [open, setOpen] = useState(true);
  void _currentUserId;
  void _initialDraft;
  void _title;
  void _titleTh;

  const headerLabel = useMemo(() => {
    if (!draftWaybillId) {
      return domain === 'expense'
        ? <Bilingual en="New expense claim" th="เบิกค่าใช้จ่ายใหม่" locale={locale} />
        : <Bilingual en="New sales order" th="เปิดใบสั่งขายใหม่" locale={locale} />;
    }
    return (
      <>
        <Bilingual en="Draft" th="ร่าง" locale={locale} />
        <span className="ml-1">· {draftWaybillId}</span>
      </>
    );
  }, [draftWaybillId, domain, locale]);

  return (
    <section
      className="glass-tint-info relative mb-8 overflow-hidden rounded-3xl border"
      aria-label={domain === 'expense' ? 'New expense claim' : 'New sales order'}
    >
      <div className="h-1 w-full bg-info rounded-t-3xl" aria-hidden />

      <header className="relative flex items-start gap-4 p-5 sm:p-6 border-b border-rule">
        <div
          className={
            'shrink-0 w-12 h-12 sm:w-14 sm:h-14 rounded-2xl flex items-center justify-center text-2xl sm:text-3xl ' +
            (domain === 'expense'
              ? 'glass-tint-accent text-accent'
              : 'bg-info text-paper')
          }
          aria-hidden
        >
          {domain === 'expense' ? '🧾' : '🛒'}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 flex-wrap">
            <h2 className="text-lg sm:text-xl font-bold text-white leading-tight">
              {headerLabel}
            </h2>
            <span
              className={
                'hidden sm:inline px-2 py-0.5 rounded-full text-sm font-mono uppercase tracking-wider ' +
                (domain === 'expense' ? 'glass-tint-info text-info' : 'bg-fuchsia-500/20 text-fuchsia-200 border border-fuchsia-500/30')
              }
            >
              {draftWaybillId
                ? <Bilingual en="Draft" th="ร่าง" locale={locale} />
                : <Bilingual en="Not started" th="ยังไม่เริ่ม" locale={locale} />}
            </span>
          </div>
          <div className="mt-1 text-sm text-ink-2 leading-relaxed">{hint}</div>
        </div>
        <div className="flex flex-col gap-2 items-end">
          {draftWaybillId && (
            <button
              type="button"
              onClick={onDiscard}
              data-testid="panel-discard-draft"
              className="glass-tint-critical rounded-lg hover:bg-critical-soft hover:border-critical/60 px-3 py-1.5 text-sm font-mono text-critical transition-colors disabled:opacity-50"
            >
              {discardLabel}
            </button>
          )}
          <button
            type="button"
            onClick={() => setOpen((o) => !o)}
            aria-expanded={open}
            aria-controls={`new-${domain}-panel-body`}
            className="glass-panel shrink-0 inline-flex items-center gap-1.5 rounded-lg border border-rule-strong hover:bg-paper-3/80 hover:border-rule-strong px-3 py-1.5 text-sm font-mono text-ink-2 transition-colors"
          >
            <span aria-hidden>{open ? '▾' : '▸'}</span>
            {open
              ? <Bilingual en="Hide" th="ซ่อน" locale={locale} />
              : <Bilingual en="Open" th="เปิด" locale={locale} />}
          </button>
        </div>
      </header>

      {open && (
        <div id={`new-${domain}-panel-body`} className="relative p-5 sm:p-6 space-y-5">
          {children}

          <button
            type="button"
            onClick={onSubmit}
            disabled={!readyToSubmit}
            data-testid="panel-submit-all"
            className={
              'w-full py-3.5 rounded-xl text-sm font-bold font-mono inline-flex items-center justify-center gap-2 shadow-lg transition-all duration-200 ' +
              'disabled:opacity-50 disabled:cursor-not-allowed ' +
              (submitting
                ? 'bg-rule-strong text-ink-2'
                : readyToSubmit
                  ? domain === 'expense'
                    ? 'bg-positive hover:bg-positive-strong text-paper'
                    : 'bg-positive hover:bg-positive-strong text-paper hover:-translate-y-px'
                  : domain === 'expense'
                    ? 'glass-panel text-mute'
                    : 'bg-slate-800 text-slate-500')
            }
          >
            {submitLabel}
          </button>
        </div>
      )}
    </section>
  );
}