'use client';

import { useEffect, useMemo, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { StepCard, StepBadge } from '@/components/StepCard';
import { CustomerCombobox, type CustomerComboboxOption } from '@/components/customer/CustomerCombobox';
import { CustomerArHistory, type CustomerArBucket } from '@/components/customer/CustomerArHistory';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { Bilingual } from '@/components/i18n/Bilingual';
import salesDict from '@folio-lib/i18n/sales';
import { NewWaybillPanel } from './NewWaybillPanel';

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

function tKey(key: string, locale: 'th' | 'de'): string {
  const en = salesDict.en[key] ?? key;
  if (locale === 'de') return salesDict.de?.[key] ?? en;
  return salesDict.th?.[key] ?? en;
}

export function NewSalesPanel({ currentUserId, initialDraft }: NewSalesPanelProps) {
  void currentUserId;
  const locale = useSecondaryLocale();
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
  const [arBuckets, setArBuckets] = useState<CustomerArBucket[] | null>(
    initialDraft?.arBuckets ?? null,
  );
  const [arTotalInvoiced, setArTotalInvoiced] = useState<number>(initialDraft?.arTotalInvoiced ?? 0);
  const [arTotalPaid, setArTotalPaid] = useState<number>(initialDraft?.arTotalPaid ?? 0);
  const [arCreditLimit, setArCreditLimit] = useState<number>(initialDraft?.arCreditLimit ?? 0);

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
      icon: '🏢',
      title: 'Pick customer',
      titleTh: 'เลือกลูกค้า',
      done: customerReady,
      active: !customerReady,
    },
    {
      key: 'items' as const,
      n: 2,
      icon: '📦',
      title: 'Line items',
      titleTh: 'รายการสินค้า',
      done: itemsReady,
      active: customerReady && !itemsReady,
    },
    {
      key: 'terms' as const,
      n: 3,
      icon: '📅',
      title: 'Payment terms & due date',
      titleTh: 'เครดิต & กำหนดชำระ',
      done: termsReady,
      active: itemsReady && !termsReady,
    },
  ];
  const completedCount = steps.filter((s) => s.done).length;

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
    // autosavePending intentionally read inside timer; we don't want it as a dep
    // (it would reset the debounce on every state flip)
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
      setSubmitError(tKey('sales.panel.pick_customer_first', locale));
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
      setSubmitError(e?.message ?? tKey('sales.panel.submit_failed', locale));
      setSubmitting(false);
    }
  }

  async function handleDiscard() {
    if (!draftWaybillId) return;
    if (!confirm(tKey('sales.panel.discard_confirm', locale))) return;
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

  const overCredit =
    arCreditLimit > 0 &&
    arTotalInvoiced - arTotalPaid >= arCreditLimit * 0.8;

  const hint = (
    <>
      <Bilingual
        en="Pick customer → add line items → set payment terms → submit for review. "
        th="เลือกลูกค้า → เพิ่มรายการ → ตั้งเครดิต → ส่งอนุมัติ "
        locale={locale}
      />
      <Bilingual
        en="sales_rep submits; sales_supervisor approves then accounting issues the invoice."
        th="sales_rep ส่ง, sales_supervisor อนุมัติ แล้วบัญชีออกใบแจ้งหนี้"
        locale={locale}
      />
      {draftWaybillId && (
        <span className="ml-2 text-xs font-mono text-mute" data-testid="autosave-status">
          {autosavePending
            ? `· ${tKey('sales.panel.saving', locale)}`
            : lastSavedAt
              ? `· ${tKey('sales.panel.saved', locale)}`
              : `· ${tKey('sales.panel.unsaved', locale)}`}
        </span>
      )}
    </>
  );

  const submitLabel = submitting
    ? <Bilingual en="⏳ Saving…" th="⏳ กำลังบันทึก…" locale={locale} />
    : canSubmitAll
      ? <Bilingual en="✓ Submit sales order for approval" th="✓ ส่งใบสั่งขายเพื่ออนุมัติ" locale={locale} />
      : <Bilingual en="🔒 Submit (disabled)" th="🔒 ส่ง (ปิดอยู่)" locale={locale} />;

  const discardLabel = <Bilingual en="🗑 Discard draft" th="🗑 ลบร่าง" locale={locale} />;

  return (
    <NewWaybillPanel
      domain="sales"
      currentUserId={currentUserId}
      initialDraft={initialDraft ?? null}
      title=""
      titleTh=""
      discardLabel={discardLabel}
      submitLabel={submitLabel}
      readyToSubmit={canSubmitAll}
      submitting={submitting}
      onSubmit={handleSubmit}
      onDiscard={handleDiscard}
      hint={hint}
      draftWaybillId={draftWaybillId}
    >
      {overCredit && customer && (
        <div className="rounded-xl border border-amber-500/40 bg-amber-950/30 px-3 py-2 text-sm font-mono text-amber-200">
          <Bilingual
            en={`⚠ Customer ${customer.code} outstanding AR is at ≥ 80% of credit limit. Credit check is more likely to require manual review.`}
            th={`⚠ ลูกค้า ${customer.code} มี AR ค้างชำระ ≥ 80% ของวงเงินเครดิต — อาจต้องตรวจสอบเครดิตด้วยตนเอง`}
            locale={locale}
          />
        </div>
      )}

      <ol className="grid grid-cols-1 sm:grid-cols-3 gap-2" aria-label="Progress steps">
        {steps.map((s) => (
          <li
            key={s.key}
            className={[
              'flex items-center gap-3 rounded-xl border px-3 py-2.5 transition-colors',
              s.done
                ? 'border-emerald-500/30 bg-emerald-500/5'
                : s.active
                ? 'border-fuchsia-500/40 bg-fuchsia-500/5'
                : 'glass-panel',
            ].join(' ')}
          >
            <StepBadge n={s.n} done={s.done} active={s.active} />
            <div className="min-w-0">
              <div className="flex items-center gap-1.5">
                <span aria-hidden className="text-sm">{s.icon}</span>
                <p className="text-xs font-bold text-white truncate">
                  <Bilingual en={s.title} th={s.titleTh} locale={locale} />
                </p>
              </div>
            </div>
          </li>
        ))}
      </ol>

      <div className="flex items-center gap-3">
        <div className="flex-1 h-1.5 rounded-full bg-slate-800/80 overflow-hidden">
          <div
            className="h-full bg-accent transition-[width] duration-500 ease-out"
            style={{ width: `${(completedCount / steps.length) * 100}%` }}
            aria-hidden
          />
        </div>
        <span className="text-sm font-mono text-slate-400 tabular-nums">
          {completedCount}/{steps.length}
        </span>
      </div>

      <StepCard
        n={1}
        icon="🏢"
        title="Pick customer"
        titleTh="เลือกลูกค้า"
        hint={
          <Bilingual
            en="Search by code or name. Newer customers with high AR utilization will be flagged."
            th="ค้นหาจากรหัสหรือชื่อ ลูกค้าที่ใช้เครดิตสูงจะถูกแจ้งเตือน"
            locale={locale}
          />
        }
        done={customerReady}
        active={!customerReady}
        tone="indigo"
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
              locale={locale}
            />
          </div>
        )}
      </StepCard>

      <StepCard
        n={2}
        icon="📦"
        title="Add line items"
        titleTh="เพิ่มรายการสินค้า/บริการ"
        hint={
          <Bilingual
            en="Each line carries description, qty, unit price, and VAT %. Subtotal + VAT roll up to the invoice total."
            th="แต่ละบรรทัดมีรายละเอียด, จำนวน, ราคาต่อหน่วย และภาษี % ยอดรวมก่อนภาษี + ภาษี จะรวมเป็นยอดใบแจ้งหนี้"
            locale={locale}
          />
        }
        done={itemsReady}
        active={customerReady && !itemsReady}
        tone="cyan"
        badge={
          itemsReady ? (
            <span className="text-sm font-mono px-2 py-0.5 rounded-full bg-emerald-500/20 text-emerald-200 border border-emerald-500/30">
              {items.length} <Bilingual en="line" th="รายการ" locale={locale} />{items.length === 1 ? '' : 's'}
            </span>
          ) : null
        }
      >
        <div className="space-y-2">
          <div className="grid grid-cols-12 gap-2 text-sm font-mono uppercase tracking-wider text-slate-500 px-1">
            <div className="col-span-6">
              <Bilingual en="description" th="รายละเอียด" locale={locale} />
            </div>
            <div className="col-span-2 text-right">
              <Bilingual en="qty" th="จำนวน" locale={locale} />
            </div>
            <div className="col-span-2 text-right">
              <Bilingual en="unit (THB)" th="หน่วย (THB)" locale={locale} />
            </div>
            <div className="col-span-1 text-right">
              <Bilingual en="VAT %" th="ภาษี %" locale={locale} />
            </div>
            <div className="col-span-1 text-right">
              <Bilingual en="line" th="รวม" locale={locale} />
            </div>
          </div>
          {items.map((it, idx) => {
            const t = lineTotals(it);
            return (
              <div
                key={it.key}
                className="glass-panel grid grid-cols-12 gap-2 items-center rounded-lg border px-2 py-2"
              >
                <input
                  type="text"
                  value={it.description}
                  onChange={(e) => changeItem(idx, { description: e.target.value })}
                  placeholder={tKey('sales.panel.item_description', locale)}
                  className="glass-panel col-span-6 rounded-md border px-2 py-1.5 text-sm text-white focus:border-fuchsia-500/60 focus:outline-none"
                />
                <input
                  type="number"
                  min={0}
                  step="0.01"
                  value={it.qty}
                  onChange={(e) =>
                    changeItem(idx, { qty: parseFloat(e.target.value) || 0 })
                  }
                  className="glass-panel col-span-2 rounded-md border px-2 py-1.5 text-right text-sm text-white tabular-nums focus:border-fuchsia-500/60 focus:outline-none"
                />
                <input
                  type="number"
                  min={0}
                  step="0.01"
                  value={it.unitPrice}
                  onChange={(e) =>
                    changeItem(idx, { unitPrice: parseFloat(e.target.value) || 0 })
                  }
                  className="glass-panel col-span-2 rounded-md border px-2 py-1.5 text-right text-sm text-white tabular-nums focus:border-fuchsia-500/60 focus:outline-none"
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
                  className="glass-panel col-span-1 rounded-md border px-2 py-1.5 text-right text-sm text-white tabular-nums focus:border-fuchsia-500/60 focus:outline-none"
                />
                <div className="col-span-1 text-right">
                  <button
                    type="button"
                    onClick={() => removeItem(idx)}
                    disabled={items.length <= 1}
                    className="glass-panel rounded-md border px-1.5 py-1 text-sm font-mono text-slate-300 hover:border-rose-500/50 hover:text-rose-200 disabled:opacity-30"
                    aria-label={`Remove line ${idx + 1}`}
                    title={`Remove line ${idx + 1}`}
                  >
                    ✕
                  </button>
                </div>
                <div className="col-span-12 -mt-1 px-1 text-right text-sm font-mono text-slate-500">
                  <Bilingual en="line" th="รายการ" locale={locale} /> {idx + 1} · {tKey('sales.panel.subtotal', locale)} {t.subtotal.toLocaleString('th-TH', { minimumFractionDigits: 2 })} · {tKey('sales.panel.vat', locale)} {t.vat.toLocaleString('th-TH', { minimumFractionDigits: 2 })} ·{' '}
                  <span className="text-emerald-300">{t.total.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB</span>
                </div>
              </div>
            );
          })}
          <button
            type="button"
            onClick={addItem}
            className="glass-panel w-full rounded-lg border border-dashed py-2 text-sm font-mono text-slate-300 hover:border-fuchsia-500/60 hover:text-fuchsia-200"
          >
            <Bilingual en="+ add line" th="+ เพิ่มรายการ" locale={locale} />
          </button>
        </div>

        <div className="mt-4 grid grid-cols-3 gap-2 text-sm font-mono">
          <div className="glass-panel rounded-lg border px-3 py-2">
            <div className="text-slate-500 uppercase tracking-wider">
              <Bilingual en="Subtotal" th="ก่อนภาษี" locale={locale} />
            </div>
            <div className="text-sm text-slate-200 tabular-nums">
              {totals.subtotal.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB
            </div>
          </div>
          <div className="glass-panel rounded-lg border px-3 py-2">
            <div className="text-slate-500 uppercase tracking-wider">
              <Bilingual en="VAT" th="ภาษี" locale={locale} />
            </div>
            <div className="text-sm text-amber-200 tabular-nums">
              {totals.vat.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB
            </div>
          </div>
          <div className="rounded-lg border border-emerald-500/40 bg-emerald-950/30 px-3 py-2">
            <div className="text-slate-300 uppercase tracking-wider">
              <Bilingual en="Total" th="รวมทั้งสิ้น" locale={locale} />
            </div>
            <div className="text-base font-bold text-emerald-300 tabular-nums">
              {totals.total.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB
            </div>
          </div>
        </div>
      </StepCard>

      <StepCard
        n={3}
        icon="📅"
        title="Payment terms & due date"
        titleTh="เครดิต & กำหนดชำระ"
        hint={
          <Bilingual
            en="sales_rep submits; auto-skips sales_review when total < 5,000 THB."
            th="sales_rep ส่ง; ข้าม sales_review อัตโนมัติเมื่อยอด < 5,000 THB"
            locale={locale}
          />
        }
        done={termsReady}
        active={itemsReady && !termsReady}
        tone="indigo"
      >
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <label className="block text-sm font-mono uppercase tracking-wider text-slate-400">
            <Bilingual en="Payment terms" th="เงื่อนไขการชำระ" locale={locale} />
            <select
              value={paymentTerms}
              onChange={(e) => {
                const next = e.target.value;
                setPaymentTerms(next);
                setDueDate(defaultDueDate(next));
              }}
              className="glass-panel mt-1 w-full rounded-md border px-2 py-2 text-sm text-white focus:border-fuchsia-500/60 focus:outline-none"
            >
              {PAYMENT_TERMS.map((term) => (
                <option key={term} value={term}>
                  {term}
                </option>
              ))}
            </select>
          </label>
          <label className="block text-sm font-mono uppercase tracking-wider text-slate-400">
            <Bilingual en="Due date" th="กำหนดชำระ" locale={locale} />
            <input
              type="date"
              value={dueDate}
              onChange={(e) => setDueDate(e.target.value)}
              className="glass-panel mt-1 w-full rounded-md border px-2 py-2 text-sm text-white focus:border-fuchsia-500/60 focus:outline-none"
            />
          </label>
        </div>
      </StepCard>

      {submitError && (
        <p className="mt-3 rounded-md border border-rose-500/40 bg-rose-950/40 px-3 py-2 text-sm text-rose-200">
          ⚠ {submitError}
        </p>
      )}
    </NewWaybillPanel>
  );
}

export default NewSalesPanel;