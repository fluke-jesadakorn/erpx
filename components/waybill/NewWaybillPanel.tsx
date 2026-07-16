'use client';

import { useMemo, useState, type ReactNode } from 'react';
import { T } from '@/components/i18n/T';
import {
  ChevronDown,
  ChevronUp,
  Trash2,
  ShoppingCart,
  Receipt,
} from 'lucide-react';

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
  headerExtra?: ReactNode;
  stickyActionBar?: ReactNode;
}

export function NewWaybillPanel({
  domain,
  currentUserId: _currentUserId,
  initialDraft: _initialDraft,
  title: _title,
  titleTh: _titleTh,
  discardLabel: _discardLabel,
  submitLabel,
  readyToSubmit,
  submitting,
  onSubmit,
  onDiscard,
  children,
  hint,
  draftWaybillId,
  headerExtra,
  stickyActionBar,
}: NewWaybillPanelProps) {
  const [open, setOpen] = useState(true);
  void _currentUserId;
  void _initialDraft;
  void _title;
  void _titleTh;
  void _discardLabel;

  const headerLabel = useMemo(() => {
    if (!draftWaybillId) {
      return domain === 'expense'
        ? <T id="waybill.expense.newClaim" />
        : <T id="waybill.sales.newOrder" />;
    }
    return (
      <>
        <T id="waybill.new.draft" />
        <span className="ml-1 text-ink-2 font-mono">· {draftWaybillId}</span>
      </>
    );
  }, [draftWaybillId, domain]);

  const isDraft = !!draftWaybillId;

  const IconDomain = domain === 'expense' ? Receipt : ShoppingCart;

  return (
    <section
      className="relative mb-8 overflow-hidden rounded-3xl glass-panel-heavy"
      aria-label={domain === 'expense' ? 'New expense claim' : 'New sales order'}
    >
      <header className="relative grid grid-cols-1 md:grid-cols-[auto_minmax(0,1fr)_auto] gap-5 md:gap-6 px-5 sm:px-7 py-5 sm:py-7 border-b border-rule">
        <div
          className={
            'hidden md:flex shrink-0 w-14 h-14 rounded-2xl items-center justify-center border ' +
            (domain === 'expense'
              ? 'bg-accent-soft text-accent border-accent/30'
              : 'bg-info-soft text-info border-info/30')
          }
          aria-hidden
        >
          <IconDomain className="size-7" strokeWidth={1.6} />
        </div>

        <div className="min-w-0 flex flex-col gap-2">
          <div className="flex items-center gap-2 flex-wrap">
            <h2 className="font-display text-xl sm:text-2xl font-semibold text-ink leading-tight tracking-tight">
              {headerLabel}
            </h2>
            <span
              aria-label={isDraft ? 'Draft' : 'Not started'}
              title={isDraft ? 'Draft · ร่าง' : 'Not started · ยังไม่เริ่ม'}
              className={
                'inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full border ' +
                (isDraft
                  ? 'border-info/40 bg-info-soft'
                  : 'border-rule bg-paper-3')
              }
            >
              <span
                aria-hidden
                className={
                  'inline-block w-1.5 h-1.5 rounded-full ' +
                  (isDraft ? 'bg-info' : 'bg-mute')
                }
              />
              <span className="sr-only">
                {isDraft ? 'Draft' : 'Not started'}
              </span>
            </span>
            {headerExtra ? <div className="ml-1">{headerExtra}</div> : null}
          </div>
          <div className="text-sm leading-relaxed text-ink-2 max-w-2xl">{hint}</div>
        </div>

        <div className="flex md:flex-col items-stretch md:items-end gap-2">
          {draftWaybillId && (
            <button
              type="button"
              onClick={onDiscard}
              data-testid="panel-discard-draft"
              title="Discard draft · ลบร่าง"
              aria-label="Discard draft"
              className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-critical/40 bg-critical-soft w-9 h-9 text-critical hover:bg-critical/15 transition-colors"
            >
              <Trash2 className="size-4" strokeWidth={2} aria-hidden />
            </button>
          )}
          <button
            type="button"
            onClick={() => setOpen((o) => !o)}
            aria-expanded={open}
            aria-controls={`new-${domain}-panel-body`}
            aria-label={open ? 'Hide panel' : 'Open panel'}
            title={open ? 'Hide · ซ่อน' : 'Open · เปิด'}
            className="inline-flex items-center justify-center rounded-lg border border-rule-strong bg-paper-3 hover:bg-paper-3/80 w-9 h-9 text-ink-2 transition-colors"
          >
            {open
              ? <ChevronUp className="size-4" strokeWidth={2} aria-hidden />
              : <ChevronDown className="size-4" strokeWidth={2} aria-hidden />}
          </button>
        </div>
      </header>

      {open && (
        <div id={`new-${domain}-panel-body`} className="relative p-5 sm:p-7 space-y-6">
          {children}

          {stickyActionBar}

          {!stickyActionBar && (
            <button
              type="button"
              onClick={onSubmit}
              disabled={!readyToSubmit}
              data-testid="panel-submit-all"
              className={
                'w-full py-3 rounded-xl text-sm font-bold font-mono inline-flex items-center justify-center gap-2 transition-colors duration-200 border ' +
                'disabled:opacity-50 disabled:cursor-not-allowed ' +
                (submitting
                  ? 'bg-rule-strong text-ink-2 border-rule-strong'
                  : readyToSubmit
                    ? 'bg-accent hover:bg-accent-strong text-paper-2 border-accent'
                    : 'bg-paper-3 text-mute border-rule-strong')
              }
            >
              {submitLabel}
            </button>
          )}
        </div>
      )}
    </section>
  );
}
