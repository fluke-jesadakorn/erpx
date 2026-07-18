'use client';

import { useEffect, useMemo, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { useTranslations } from 'next-intl';
import {
  Building2,
  Package,
  CalendarClock,
  Banknote,
  CircleAlert,
  Check,
  CircleDot,
  Circle,
  ArrowRight,
  ArrowUpRight,
  Loader2,
  Lock,
  CreditCard,
  CalendarDays,
  TriangleAlert,
  CircleCheck,
} from 'lucide-react';
import { StepCard, StepBadge } from '@/components/StepCard';
import { CustomerCombobox, type CustomerComboboxOption } from '@/components/customer/CustomerCombobox';
import { CustomerArHistory, type CustomerArBucket } from '@/components/customer/CustomerArHistory';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { T } from '@/components/i18n/T';
import { Modal } from '@/components/ui';
import { NewWaybillPanel } from './NewWaybillPanel';
import { SalesExtractPanel, type ExtractedDraft } from './SalesExtractPanel';

export interface SoItemDraft {
  key: string;
  description: string;
  qty: number;
  unitPrice: number;
  vatPct: number;
}

export interface NewSalesPanelProps {
  currentUserId: number;
  initialDraft?:
    | {
        waybillId: string;
        salesOrderId: number | null;
        customerId?: number | null;
        customerName?: string | null;
        paymentTerms?: string | null;
        dueDate?: string | null;
        items?: SoItemDraft[] | null;
        arBuckets?: CustomerArBucket[] | null;
        arTotalInvoiced?: number;
        arTotalPaid?: number;
        arCreditLimit?: number;
        savedAt?: string | null;
      }
    | null;
}

const AUTOSAVE_DEBOUNCE_MS = 10_000;

const PAYMENT_TERMS = ['Net 15', 'Net 30', 'Net 45', 'Net 60', 'Cash on delivery', 'Prepaid'];

const SO_INPUT =
  'rounded-lg border border-rule bg-paper-2 px-3 py-2 text-sm text-ink placeholder:text-mute/60 transition-colors focus:outline-none focus:border-accent focus:ring-2 focus:ring-accent/25 disabled:opacity-50';
const SO_INPUT_NUM =
  'rounded-lg border border-rule bg-paper-2 px-3 py-2 text-sm text-ink tabular-nums placeholder:text-mute/60 transition-colors focus:outline-none focus:border-accent focus:ring-2 focus:ring-accent/25 disabled:opacity-50';
const SO_INPUT_ERR =
  'rounded-lg border border-critical bg-paper-2 px-3 py-2 text-sm text-ink placeholder:text-mute/60 transition-colors focus:outline-none focus:border-critical focus:ring-2 focus:ring-critical/25 disabled:opacity-50';

function uid(): string {
  return Math.random().toString(36).slice(2, 10);
}

function round2(n: number): number {
  return Math.round((Number(n) || 0) * 100) / 100;
}

function lineTotals(it: SoItemDraft): { subtotal: number; vat: number; total: number } {
  const subtotal = round2(it.qty * it.unitPrice);
  const vat = round2((subtotal * (it.vatPct || 0)) / 100);
  return { subtotal, vat, total: round2(subtotal + vat) };
}

function summarize(items: SoItemDraft[]): {
  subtotal: number;
  vat: number;
  total: number;
} {
  let subtotal = 0;
  let vat = 0;
  for (const it of items) {
    const t = lineTotals(it);
    subtotal += t.subtotal;
    vat += t.vat;
  }
  subtotal = round2(subtotal);
  vat = round2(vat);
  return { subtotal, vat, total: round2(subtotal + vat) };
}

function defaultDueDate(paymentTerms: string): string {
  const days = paymentTerms.startsWith('Net ')
    ? parseInt(paymentTerms.slice(4), 10) || 30
    : paymentTerms === 'Cash on delivery' || paymentTerms === 'Prepaid'
    ? 0
    : 30;
  const d = new Date();
  d.setDate(d.getDate() + days);
  return d.toISOString().slice(0, 10);
}

function isDraftShape(x: unknown): x is { waybillId: string } {
  return !!x && typeof x === 'object' && typeof (x as any).waybillId === 'string';
}

function camel(s: string): string {
  return s.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
}

function panelKey(snake: string): string {
  const cleaned = snake.replace(/^sales\./, '');
  return `sales.${camel(cleaned)}`;
}

function fmtShortDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '—';
  const dd = String(d.getDate()).padStart(2, '0');
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  return `${dd}/${mm}`;
}

export function NewSalesPanel({ currentUserId, initialDraft }: NewSalesPanelProps) {
  void currentUserId;
  const locale = useSecondaryLocale();
  const tc = useTranslations();
  function tKey(key: string): string {
    const id = panelKey(key);
    try { return tc(id); } catch { return id; }
  }
  const initialItems = useMemo<SoItemDraft[]>(
    () =>
      initialDraft?.items && initialDraft.items.length > 0
        ? initialDraft.items.map((x) => ({ ...x, key: x.key || uid() }))
        : [{ key: uid(), description: '', qty: 1, unitPrice: 0, vatPct: 7 }],
    [initialDraft],
  );
  const safeInitial = isDraftShape(initialDraft ?? {}) ? initialDraft : null;

  const [customer, setCustomer] = useState<CustomerComboboxOption | null>(
    initialDraft?.customerId
      ? {
          id: initialDraft.customerId,
          code: '',
          name: initialDraft.customerName ?? '',
        }
      : null,
  );
  const [items, setItems] = useState<SoItemDraft[]>(initialItems);
  const [paymentTerms, setPaymentTerms] = useState<string>(
    initialDraft?.paymentTerms ?? 'Net 30',
  );
  const [dueDate, setDueDate] = useState<string>(
    initialDraft?.dueDate ?? defaultDueDate(initialDraft?.paymentTerms ?? 'Net 30'),
  );
  const [draftWaybillId, setDraftWaybillId] = useState<string | null>(
    safeInitial?.waybillId ?? null,
  );
  const [lastSavedAt, setLastSavedAt] = useState<string | null>(
    safeInitial?.savedAt ?? null,
  );
  const [autosavePending, setAutosavePending] = useState<boolean>(false);
  const [submitting, setSubmitting] = useState<boolean>(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [confirmDiscard, setConfirmDiscard] = useState<boolean>(false);
  const [arBuckets, setArBuckets] = useState<CustomerArBucket[] | null>(
    initialDraft?.arBuckets ?? null,
  );
  const [arTotalInvoiced, setArTotalInvoiced] = useState<number>(initialDraft?.arTotalInvoiced ?? 0);
  const [arTotalPaid, setArTotalPaid] = useState<number>(initialDraft?.arTotalPaid ?? 0);
  const [arCreditLimit, setArCreditLimit] = useState<number>(initialDraft?.arCreditLimit ?? 0);
  const [dirty, setDirty] = useState<Record<string, boolean>>({});
  const markDirty = (k: string) =>
    setDirty((prev) => (prev[k] ? prev : { ...prev, [k]: true }));
  const fieldErr = (k: string, isInvalid: boolean) => dirty[k] === true && isInvalid;

  const router = useRouter();
  const draftRef = useRef({ waybillId: draftWaybillId });
  draftRef.current = { waybillId: draftWaybillId };

  const totals = useMemo(() => summarize(items), [items]);

  const customerReady = !!customer;
  const itemsReady =
    items.length > 0 &&
    items.every(
      (it) => it.description.trim().length > 0 && it.qty > 0 && it.unitPrice >= 0,
    );
  const termsReady = paymentTerms.length > 0 && dueDate.length === 10;
  const canSubmitAll = customerReady && itemsReady && termsReady && !submitting;

  const steps = [
    {
      key: 'customer' as const,
      n: 1,
      Icon: Building2,
      title: 'Pick customer',
      titleTh: 'เลือกลูกค้า',
      done: customerReady,
      active: !customerReady,
    },
    {
      key: 'items' as const,
      n: 2,
      Icon: Package,
      title: 'Line items',
      titleTh: 'รายการสินค้า',
      done: itemsReady,
      active: customerReady && !itemsReady,
    },
    {
      key: 'terms' as const,
      n: 3,
      Icon: CalendarClock,
      title: 'Payment terms',
      titleTh: 'เครดิต & กำหนดชำระ',
      done: termsReady,
      active: itemsReady && !termsReady,
    },
  ];
  const completedCount = steps.filter((s) => s.done).length;
  const blocker = !customerReady
    ? {
        Icon: Building2,
        en: 'Pick a customer',
        th: 'เลือกลูกค้า',
      }
    : !itemsReady
    ? { Icon: Package, en: 'Add at least 1 line item', th: 'เพิ่มรายการสินค้า' }
    : !termsReady
    ? { Icon: CalendarClock, en: 'Set due date', th: 'ตั้งกำหนดชำระ' }
    : null;

  function scrollToStep(n: number) {
    if (typeof document === 'undefined') return;
    document.getElementById(`step-${n}`)?.scrollIntoView({
      behavior: 'smooth',
      block: 'start',
    });
  }

  useEffect(() => {
    if (!customer) {
      setArBuckets(null);
      setArTotalInvoiced(0);
      setArTotalPaid(0);
      setArCreditLimit(0);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const r = await fetch(`/api/customers/${customer.id}/ar-history`, {
          credentials: 'include',
        });
        if (!r.ok) return;
        const data = await r.json().catch(() => null);
        if (cancelled) return;
        setArBuckets((data?.buckets ?? null) as CustomerArBucket[] | null);
        setArTotalInvoiced(Number(data?.totalInvoiced ?? data?.total_invoiced ?? 0));
        setArTotalPaid(Number(data?.totalPaid ?? data?.total_paid ?? 0));
        setArCreditLimit(Number(data?.creditLimit ?? data?.credit_limit ?? 0));
      } catch {
        if (!cancelled) return;
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [customer]);

  useEffect(() => {
    return () => {
      const waybillId = draftRef.current.waybillId;
      if (!waybillId) return;
      const fd = new FormData();
      fd.set('waybillId', waybillId);
      if (typeof navigator !== 'undefined' && typeof navigator.sendBeacon === 'function') {
        navigator.sendBeacon('/api/sales/discard-draft', fd);
      }
    };
  }, []);

  const seedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (!customerReady || draftWaybillId) return;
    if (seedTimerRef.current) clearTimeout(seedTimerRef.current);
    seedTimerRef.current = setTimeout(async () => {
      if (draftRef.current.waybillId) return;
      try {
        const r = await fetch('/api/sales/start-draft', {
          method: 'POST',
          credentials: 'include',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({
            customerId: customer?.id ?? null,
            paymentTerms,
            dueDate,
          }),
        });
        const data = await r.json().catch(() => null);
        if (!r.ok || !data?.waybillId) return;
        setDraftWaybillId(data.waybillId);
        setLastSavedAt(new Date().toISOString());
      } catch {
        return;
      }
    }, 1_500);
    return () => {
      if (seedTimerRef.current) clearTimeout(seedTimerRef.current);
    };
  }, [customerReady, customer, paymentTerms, dueDate, draftWaybillId]);

  useEffect(() => {
    if (!draftWaybillId) return;
    const timer = setTimeout(() => {
      if (autosavePending) return;
      void (async () => {
        setAutosavePending(true);
        try {
          const r = await fetch('/api/sales/save-draft', {
            method: 'POST',
            credentials: 'include',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
              waybillId: draftWaybillId,
              customerId: customer?.id ?? null,
              paymentTerms,
              dueDate,
              items: items.map((it) => ({
                description: it.description,
                qty: it.qty,
                unitPrice: it.unitPrice,
                vatPct: it.vatPct,
              })),
              totals,
            }),
          });
          const data = await r.json().catch(() => null);
          if (r.ok && data?.savedAt) setLastSavedAt(data.savedAt);
        } finally {
          setAutosavePending(false);
        }
      })();
    }, AUTOSAVE_DEBOUNCE_MS);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draftWaybillId, customer, paymentTerms, dueDate, items, totals]);

  function changeItem(idx: number, patch: Partial<SoItemDraft>) {
    setItems((prev) =>
      prev.map((x, i) => (i === idx ? { ...x, ...patch } : x)),
    );
  }

  function addItem() {
    setItems((prev) => [
      ...prev,
      { key: uid(), description: '', qty: 1, unitPrice: 0, vatPct: 7 },
    ]);
  }

  function removeItem(idx: number) {
    setItems((prev) => prev.filter((_, i) => i !== idx));
  }

  async function handleSubmit() {
    if (!customer) {
      setSubmitError(tKey('sales.panel.pick_customer_first'));
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      const r = await fetch('/api/sales', {
        method: 'POST',
        credentials: 'include',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          draftWaybillId,
          customerId: customer.id,
          paymentTerms,
          dueDate,
          items: items.map((it) => ({
            description: it.description,
            qty: it.qty,
            unitPrice: it.unitPrice,
            vatPct: it.vatPct,
          })),
          totals,
        }),
      });
      const data = await r.json().catch(() => null);
      if (!r.ok) {
        setSubmitError(data?.error ?? `HTTP ${r.status}`);
        setSubmitting(false);
        return;
      }
      const target = data?.waybillId ?? draftRef.current.waybillId;
      if (target) {
        router.push(`/waybill/${target}`);
      } else {
        router.refresh();
      }
    } catch (e: any) {
      setSubmitError(e?.message ?? tKey('sales.panel.submit_failed'));
      setSubmitting(false);
    }
  }

  async function handleDiscard() {
    if (!draftWaybillId) return;
    try {
      const fd = new FormData();
      fd.set('waybillId', draftWaybillId);
      const url = '/api/sales/discard-draft';
      if (typeof navigator !== 'undefined' && typeof navigator.sendBeacon === 'function') {
        navigator.sendBeacon(url, fd);
      } else {
        await fetch(url, { method: 'POST', body: fd, credentials: 'include' });
      }
      setDraftWaybillId(null);
      setLastSavedAt(null);
      router.refresh();
    } catch {
      return;
    }
  }

  const applyDraft = (d: ExtractedDraft) => {
    if (d.customer_id || d.customer_name || d.customer_code) {
      setCustomer({
        id: d.customer_id ?? 0,
        code: d.customer_code ?? '',
        name: d.customer_name ?? '',
      });
    }
    if (Array.isArray(d.items) && d.items.length > 0) {
      setItems(
        d.items.map((it) => ({
          key: uid(),
          description: String(it.description ?? ''),
          qty: Number(it.qty ?? 0) || 0,
          unitPrice: Number(it.unit_price ?? 0) || 0,
          vatPct: 7,
        })),
      );
    }
    if (d.payment_terms && PAYMENT_TERMS.includes(d.payment_terms)) {
      setPaymentTerms(d.payment_terms);
      setDueDate(defaultDueDate(d.payment_terms));
    }
  };

  const overCredit =
    arCreditLimit > 0 &&
    arTotalInvoiced - arTotalPaid >= arCreditLimit * 0.8;

  const overCreditPct =
    arCreditLimit > 0
      ? Math.round(((arTotalInvoiced - arTotalPaid) / arCreditLimit) * 100)
      : 0;

  const hint = (
    <span
      title="Pick customer → add line items → set payment terms → submit for review · เลือกลูกค้า → เพิ่มรายการ → ตั้งเครดิต → ส่งอนุมัติ"
    >
      <T id="waybill.sales.pick_add_set_submit" />
      {draftWaybillId && (
        <span className="ml-2 text-xs font-mono text-mute" data-testid="autosave-status">
          · {autosavePending
            ? tKey('sales.panel.saving')
            : lastSavedAt
              ? tKey('sales.panel.saved')
              : tKey('sales.panel.unsaved')}
        </span>
      )}
    </span>
  );

  const submitLabel = submitting ? (
    <span className="inline-flex items-center gap-2">
      <Loader2 className="size-4 animate-spin" aria-hidden />
      <T id="waybill.sales.saving" />
    </span>
  ) : canSubmitAll ? (
    <span className="inline-flex items-center gap-2">
      <ArrowRight className="size-4" aria-hidden />
      <T id="waybill.sales.submit" />
    </span>
  ) : (
    <span className="inline-flex items-center gap-2">
      <Lock className="size-4" aria-hidden />
      <T id="waybill.sales.submit" />
    </span>
  );

  return (
    <NewWaybillPanel
      domain="sales"
      currentUserId={currentUserId}
      initialDraft={initialDraft ?? null}
      title=""
      titleTh=""
      discardLabel={null}
      submitLabel={submitLabel}
      readyToSubmit={canSubmitAll}
      submitting={submitting}
      onSubmit={handleSubmit}
      onDiscard={() => setConfirmDiscard(true)}
      hint={hint}
      draftWaybillId={draftWaybillId}
      headerExtra={
        <span
          aria-live="polite"
          title={
            totals.total > 0
              ? `${totals.total.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB`
              : '—'
          }
          className={[
            'inline-flex items-center gap-1.5 rounded-md border px-2.5 py-0.5 text-sm font-mono font-semibold tabular-nums',
            canSubmitAll
              ? 'border-positive/40 bg-positive-soft text-positive'
              : totals.total > 0
              ? 'border-info/40 bg-info-soft text-info'
              : 'border-rule bg-paper-3 text-mute',
          ].join(' ')}
          data-testid="sales-header-total"
        >
          <Banknote className="size-3.5" aria-hidden strokeWidth={2} />
          {totals.total > 0
            ? `${totals.total.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB`
            : '— THB'}
        </span>
      }
      stickyActionBar={
        <div
          className="sticky bottom-2 z-10 -mx-5 sm:-mx-7 px-5 sm:px-7 py-4 bg-paper-2 rounded-md border border-rule"
          data-testid="sales-sticky-bar"
        >
          <div className="grid grid-cols-1 sm:grid-cols-[minmax(0,1fr)_auto] items-center gap-3 sm:gap-5">
            <div className="flex min-w-0 items-center gap-2 text-sm">
              {blocker ? (
                <span
                  title={`${blocker.en} · ${blocker.th}`}
                  aria-label={blocker.en}
                  className="inline-flex shrink-0 w-9 h-9 items-center justify-center rounded-full bg-caution-soft text-caution border border-caution/40"
                >
                  <blocker.Icon className="size-4" strokeWidth={2} aria-hidden />
                </span>
              ) : (
                <>
                  <span
                    aria-label="Ready"
                    title="Ready · พร้อม"
                    className="inline-flex shrink-0 w-9 h-9 items-center justify-center rounded-full bg-positive-soft text-positive border border-positive/40"
                  >
                    <CircleCheck className="size-4" strokeWidth={2.5} aria-hidden />
                  </span>
                  <div className="flex items-center gap-1.5 min-w-0 flex-wrap">
                    <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md border border-rule bg-paper-3 text-xs font-mono tabular-nums text-ink-2">
                      <Package className="size-3" aria-hidden strokeWidth={2} />
                      {items.length}
                    </span>
                    <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md border border-rule bg-paper-3 text-xs font-mono tabular-nums text-ink-2">
                      <CalendarDays className="size-3" aria-hidden strokeWidth={2} />
                      {fmtShortDate(dueDate)}
                    </span>
                    <span
                      title={`${paymentTerms} · เงื่อนไขการชำระ`}
                      className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md border border-rule bg-paper-3 text-xs font-mono tabular-nums text-ink-2"
                    >
                      <CreditCard className="size-3" aria-hidden strokeWidth={2} />
                      {paymentTerms.replace('Net ', 'N')}
                    </span>
                  </div>
                </>
              )}
            </div>
            <div className="hidden sm:block w-px h-12 bg-rule" aria-hidden />
            <div className="flex items-center gap-4 justify-self-stretch sm:justify-self-end">
              <div
                aria-live="polite"
                title={
                  totals.total > 0
                    ? `${totals.total.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB · รวม`
                    : '—'
                }
                className={[
                  'font-display text-2xl font-semibold tabular-nums leading-none',
                  canSubmitAll ? 'text-positive' : 'text-ink',
                ].join(' ')}
                data-testid="sales-sticky-total"
              >
                {totals.total > 0
                  ? totals.total.toLocaleString('th-TH', { minimumFractionDigits: 2 })
                  : '—'}
                <span className="ml-1 text-sm font-mono font-normal text-mute">THB</span>
              </div>
              <button
                type="button"
                onClick={handleSubmit}
                disabled={!canSubmitAll}
                title={
                  canSubmitAll
                    ? 'Submit for approval · ส่งเพื่ออนุมัติ'
                    : 'Disabled · ปิดอยู่'
                }
                data-testid="sales-sticky-submit"
                className={[
                  'shrink-0 inline-flex items-center justify-center gap-2 rounded-md border w-12 h-12 transition-all duration-200',
                  'disabled:opacity-50 disabled:cursor-not-allowed',
                  submitting
                    ? 'bg-rule-strong text-ink-2 border-rule-strong'
                    : canSubmitAll
                    ? 'bg-accent hover:bg-accent-strong text-paper-2 border-accent shadow-[0_8px_24px_-8px_color-mix(in_oklab,var(--accent)_55%,transparent)]'
                    : 'bg-paper-3 text-mute border-rule-strong',
                ].join(' ')}
              >
                {submitting ? (
                  <Loader2 className="size-5 animate-spin" aria-hidden />
                ) : canSubmitAll ? (
                  <ArrowUpRight className="size-5" aria-hidden strokeWidth={2.5} />
                ) : (
                  <Lock className="size-5" aria-hidden strokeWidth={2} />
                )}
                <span className="sr-only">
                  {canSubmitAll ? 'Submit' : 'Submit (locked)'}
                </span>
              </button>
            </div>
          </div>
        </div>
      }
    >
      {overCredit && customer && (
        <div
          title={`Customer ${customer.code} outstanding AR is at ≥ 80% of credit limit · ลูกค้า ${customer.code} มี AR ค้างชำระ ≥ 80% ของวงเงินเครดิต — อาจต้องตรวจสอบเครดิตด้วยตนเอง`}
          className="inline-flex items-center gap-1.5 rounded-md border border-caution/40 bg-caution-soft px-2.5 py-1 text-xs font-mono text-caution self-start"
        >
          <TriangleAlert className="size-3.5" aria-hidden strokeWidth={2.5} />
          <span>
            {customer.code} · {overCreditPct}%
          </span>
        </div>
      )}

      <SalesExtractPanel lang={locale} onUse={applyDraft} />

      <div className="flex items-center justify-end" aria-label="Progress">
        <span className="text-xs font-mono tabular-nums text-mute">
          {completedCount}/{steps.length}
        </span>
      </div>
      <ol className="relative grid grid-cols-3 gap-y-3">
        {steps.map((s, i) => {
          const Icon = s.Icon;
          return (
            <li key={s.key} className="relative flex justify-center">
              {i < steps.length - 1 && (
                <span
                  aria-hidden
                  className="hidden sm:block absolute top-[18px] left-[calc(50%+22px)] right-[calc(-50%+22px)] h-px bg-rule overflow-hidden"
                >
                  <span
                    className="block h-full bg-positive transition-[width] duration-500 ease-out"
                    style={{ width: completedCount > i ? '100%' : '0%' }}
                  />
                </span>
              )}
              <button
                type="button"
                onClick={() => scrollToStep(s.n)}
                aria-current={s.active ? 'step' : undefined}
                aria-label={`Jump to step ${s.n}: ${s.title}`}
                title={`${s.title} · ${s.titleTh}`}
                className="group flex flex-col items-center gap-1.5 w-full sm:w-auto px-2 py-2 transition-colors"
              >
                <StepBadge n={s.n} done={s.done} active={s.active} tone="accent" />
                <Icon
                  aria-hidden
                  className={[
                    'size-4 mt-0.5',
                    s.done
                      ? 'text-positive'
                      : s.active
                      ? 'text-accent'
                      : 'text-mute',
                  ].join(' ')}
                  strokeWidth={2}
                />
                <p
                  className={[
                    'text-xs truncate max-w-full text-center',
                    s.done || s.active ? 'text-ink font-semibold' : 'text-ink-2',
                  ].join(' ')}
                >
                  {s.title}
                </p>
                <span aria-hidden className="text-mute">
                  {s.done ? (
                    <Check className="size-3 text-positive" strokeWidth={3} />
                  ) : s.active ? (
                    <CircleDot className="size-3 text-accent" strokeWidth={3} />
                  ) : (
                    <Circle className="size-3 text-mute" strokeWidth={2} />
                  )}
                </span>
              </button>
            </li>
          );
        })}
      </ol>

      <StepCard
        n={1}
        icon={<Building2 className="size-4" strokeWidth={2} aria-hidden />}
        title={
          <T id="waybill.sales.pick_customer" />
        }
        hint={
          <span title="Search by code or name. Newer customers with high AR utilization will be flagged · ค้นหาจากรหัสหรือชื่อ ลูกค้าที่ใช้เครดิตสูงจะถูกแจ้งเตือน">
            <T id="waybill.sales.search_by_code_or_name" />
          </span>
        }
        cardId="step-1"
        done={customerReady}
        active={!customerReady}
        tone="accent"
        bodyTint
        flat
      >
        <CustomerCombobox
          value={customer?.id ?? null}
          onChange={(id, c) => setCustomer(c ?? (id != null ? { id, code: '', name: '' } : null))}
        />
        {customer && (
          <div className="mt-4">
            <CustomerArHistory
              data={arBuckets}
              totalInvoiced={arTotalInvoiced}
              totalPaid={arTotalPaid}
              creditLimit={arCreditLimit}
              customerName={customer.name}
            />
          </div>
        )}
      </StepCard>

      <StepCard
        n={2}
        icon={<Package className="size-4" strokeWidth={2} aria-hidden />}
        title={
          <T id="waybill.sales.line_items" />
        }
        hint={
          <span title="Each line carries description, qty, unit price, and VAT % · แต่ละบรรทัดมีรายละเอียด, จำนวน, ราคาต่อหน่วย และภาษี %">
            <T id="waybill.sales.description_qty_price_vat" />
          </span>
        }
        cardId="step-2"
        done={itemsReady}
        active={customerReady && !itemsReady}
        tone="accent"
        bodyTint
        flat
        badge={
          itemsReady ? (
            <span className="text-xs font-mono px-2 py-0.5 rounded-full bg-positive-soft text-positive border border-positive/40 inline-flex items-center gap-1">
              <CircleCheck className="size-3" aria-hidden strokeWidth={2.5} />
              {items.length}
            </span>
          ) : null
        }
      >
        <div className="space-y-2">
          <div className="grid grid-cols-12 gap-2 text-xs font-mono uppercase tracking-wider text-ink-2 px-1">
            <div className="col-span-5">
              <T id="waybill.sales.description" />
            </div>
            <div className="col-span-2 text-right">
              <T id="waybill.sales.qty" />
            </div>
            <div className="col-span-2 text-right">
              <T id="waybill.sales.unit" />
            </div>
            <div className="col-span-1 text-right">
              <T id="waybill.sales.vat" />
            </div>
            <div className="col-span-2 text-right">
              <T id="waybill.sales.line" />
            </div>
          </div>
          {items.map((it, idx) => {
            const t = lineTotals(it);
            const showDescErr = fieldErr(`${it.key}-desc`, !it.description.trim());
            const showQtyErr = fieldErr(`${it.key}-qty`, it.qty <= 0);
            return (
              <div key={it.key} className="space-y-1">
                <div className="grid grid-cols-12 gap-2 items-center rounded-md border border-rule bg-paper-3 px-2 py-2">
                  <input
                    type="text"
                    value={it.description}
                    onChange={(e) => {
                      markDirty(`${it.key}-desc`);
                      changeItem(idx, { description: e.target.value });
                    }}
                    placeholder={tKey('sales.panel.item_description')}
                    aria-invalid={showDescErr || undefined}
                    className={`col-span-5 ${showDescErr ? SO_INPUT_ERR : SO_INPUT}`}
                  />
                  <input
                    type="number"
                    min={0}
                    step="0.01"
                    value={it.qty}
                    onChange={(e) => {
                      markDirty(`${it.key}-qty`);
                      changeItem(idx, { qty: parseFloat(e.target.value) || 0 });
                    }}
                    aria-invalid={showQtyErr || undefined}
                    className={`col-span-2 text-right ${showQtyErr ? SO_INPUT_ERR : SO_INPUT_NUM}`}
                  />
                  <input
                    type="number"
                    min={0}
                    step="0.01"
                    value={it.unitPrice}
                    onChange={(e) =>
                      changeItem(idx, { unitPrice: parseFloat(e.target.value) || 0 })
                    }
                    className={`col-span-2 text-right ${SO_INPUT_NUM}`}
                  />
                  <input
                    type="number"
                    min={0}
                    max={100}
                    step="0.01"
                    value={it.vatPct}
                    onChange={(e) =>
                      changeItem(idx, { vatPct: parseFloat(e.target.value) || 0 })
                    }
                    className={`col-span-1 text-right ${SO_INPUT_NUM}`}
                  />
                  <div
                    aria-live="polite"
                    className={[
                      'col-span-1 self-center text-right text-sm font-mono font-bold tabular-nums',
                      t.total > 0 ? 'text-positive' : 'text-mute',
                    ].join(' ')}
                  >
                    {t.total.toLocaleString('th-TH', { minimumFractionDigits: 2 })}
                  </div>
                  <div className="col-span-1 text-right">
                    <button
                      type="button"
                      onClick={() => removeItem(idx)}
                      disabled={items.length <= 1}
                      className="rounded-md border border-rule bg-paper-2 px-1.5 py-1 text-xs font-mono text-ink-2 hover:border-critical hover:text-critical disabled:opacity-30"
                      aria-label={`Remove line ${idx + 1}`}
                      title={`Remove line ${idx + 1}`}
                    >
                      ✕
                    </button>
                  </div>
                </div>
                {(showDescErr || showQtyErr) && (
                  <p className="px-2 text-xs font-mono text-critical">
                    {showDescErr && showQtyErr
                      ? <T id="waybill.sales.description_required_qty_must_be_0" />
                      : showDescErr
                      ? <T id="waybill.sales.description_required" />
                      : <T id="waybill.sales.qty_must_be_0" />}
                  </p>
                )}
              </div>
            );
          })}
          <button
            type="button"
            onClick={addItem}
            className="w-full rounded-lg border border-dashed border-rule bg-paper-3 py-2 text-sm font-mono text-ink-2 hover:border-accent hover:text-accent transition-colors inline-flex items-center justify-center gap-1.5"
          >
            <span aria-hidden>+</span>
            <T id="waybill.sales.add_line" />
          </button>
        </div>

        <div className="mt-4 grid grid-cols-3 gap-2 text-xs font-mono">
          <div className="rounded-lg border border-rule bg-paper-3 px-3 py-2">
            <div className="text-mute uppercase tracking-wider text-xs">
              <T id="waybill.sales.subtotal" />
            </div>
            <div className="text-sm text-ink tabular-nums mt-0.5">
              {totals.subtotal.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB
            </div>
          </div>
          <div className="rounded-lg border border-rule bg-paper-3 px-3 py-2">
            <div className="text-mute uppercase tracking-wider text-xs">
              <T id="waybill.sales.vat" />
            </div>
            <div className="text-sm text-caution tabular-nums mt-0.5">
              {totals.vat.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB
            </div>
          </div>
          <div className="rounded-lg border border-positive/40 bg-positive-soft px-3 py-2">
            <div className="text-positive uppercase tracking-wider text-xs">
              <T id="waybill.sales.total" />
            </div>
            <div className="text-lg font-bold text-positive tabular-nums mt-0.5">
              {totals.total.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB
            </div>
          </div>
        </div>
      </StepCard>

      <StepCard
        n={3}
        icon={<CalendarClock className="size-4" strokeWidth={2} aria-hidden />}
        title={
          <T id="waybill.sales.payment_terms" />
        }
        hint={
          <span title="sales_rep submits; auto-skips sales_review when total < 5,000 THB · sales_rep ส่ง; ข้าม sales_review อัตโนมัติเมื่อยอด < 5,000 THB">
            <T id="waybill.sales.auto_skip_review_under_5_000" />
          </span>
        }
        cardId="step-3"
        done={termsReady}
        active={itemsReady && !termsReady}
        tone="accent"
        bodyTint
        flat
      >
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <label className="block text-xs font-mono uppercase tracking-wider text-ink-2">
            <span className="inline-flex items-center gap-1.5">
              <CreditCard className="size-3.5" aria-hidden strokeWidth={2} />
              <T id="waybill.sales.terms" />
            </span>
            <select
              value={paymentTerms}
              onChange={(e) => {
                const next = e.target.value;
                setPaymentTerms(next);
                setDueDate(defaultDueDate(next));
              }}
              className={`mt-1.5 w-full ${SO_INPUT}`}
            >
              {PAYMENT_TERMS.map((term) => (
                <option key={term} value={term}>
                  {term}
                </option>
              ))}
            </select>
          </label>
          <label className="block text-xs font-mono uppercase tracking-wider text-ink-2">
            <span className="inline-flex items-center gap-1.5">
              <CalendarDays className="size-3.5" aria-hidden strokeWidth={2} />
              <T id="waybill.sales.due" />
            </span>
            <input
              type="date"
              value={dueDate}
              onChange={(e) => {
                markDirty('due');
                setDueDate(e.target.value);
              }}
              aria-invalid={fieldErr('due', !termsReady) || undefined}
              className={`mt-1.5 w-full ${fieldErr('due', !termsReady) ? SO_INPUT_ERR : SO_INPUT}`}
            />
          </label>
        </div>
      </StepCard>

      {submitError && (
        <p className="mt-3 rounded-md border border-critical/40 bg-critical-soft px-3 py-2 text-xs text-critical inline-flex items-center gap-2">
          <CircleAlert className="size-3.5" aria-hidden strokeWidth={2.5} />
          {submitError}
        </p>
      )}

      <Modal
        open={confirmDiscard}
        onClose={() => setConfirmDiscard(false)}
        title={tKey('sales.panel.discard_confirm')}
        tone="rose"
        width="md"
        footer={
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setConfirmDiscard(false)}
              className="rounded-md border border-rule bg-paper-2 px-3 py-1.5 text-xs font-mono text-ink-2 hover:bg-paper-3 transition-colors"
            >
              <T id="common.cancel" />
            </button>
            <button
              type="button"
              onClick={() => {
                setConfirmDiscard(false);
                void handleDiscard();
              }}
              className="rounded-md bg-critical px-3 py-1.5 text-xs font-mono text-paper-2 hover:bg-critical-strong transition-colors"
            >
              <T id="common.delete" />
            </button>
          </div>
        }
      >
        <p className="text-sm text-ink-2">
          {draftWaybillId && (
            <>
              <span className="font-mono text-ink">{draftWaybillId}</span> ·{' '}
            </>
          )}
          The draft will be removed from the server.
        </p>
      </Modal>
    </NewWaybillPanel>
  );
}

export default NewSalesPanel;
