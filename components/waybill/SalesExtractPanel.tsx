'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { recordSalesPaymentAction } from '@/app/actions/sales';
import { AiCoaChip } from '@/components/waybill/AiCoaChip';
import { SalesPaymentSlipUpload } from '@/components/waybill/SalesPaymentSlipUpload';
import { T } from '@/components/i18n/T';

export interface ExtractedDraft {
  customer_name: string;
  customer_code: string | null;
  customer_id: number | null;
  payment_terms: string;
  items: Array<{ description: string; qty: number; unit_price: number }>;
  confidence: number;
}

export interface SoItemRow {
  id: number;
  description: string;
  qty: number;
  unit_price: number;
  vat_amount: number;
  line_total: number;
  mapped_revenue_account_code: string | null;
  confidence_score: number | null;
}

interface Props {
  lang: 'en' | 'th' | 'de';
  onUse: (draft: ExtractedDraft) => void;
  waybillId?: string;
  soId?: number;
  soItems?: SoItemRow[];
  existingArSlipId?: number | null;
  canRecord?: boolean;
}

export function SalesExtractPanel({
  lang,
  onUse,
  waybillId,
  soId,
  soItems = [],
  existingArSlipId = null,
  canRecord = false,
}: Props) {
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [draft, setDraft] = useState<ExtractedDraft | null>(null);
  const [error, setError] = useState<string | null>(null);
  const router = useRouter();

  const onExtract = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    setError(null);
    setDraft(null);
    try {
      const res = await fetch('/api/sales/extract', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ text, lang }),
      });
      const json = await res.json();
      if (!res.ok || !json.ok) {
        setError(json.error ?? `HTTP ${res.status}`);
      } else if (json.draft) {
        setDraft(json.draft);
      } else {
        setError('AI could not extract a draft from this text.');
      }
    } catch (e: any) {
      setError(e?.message ?? String(e));
    } finally {
      setBusy(false);
    }
  };

  const showRecordingSections =
    waybillId != null && soId != null && soItems.length > 0;

  return (
    <>
      <section className="mt-4 rounded-2xl border border-fuchsia-500/30 bg-fuchsia-950/15 p-4">
        <h3 className="mb-1 text-xs font-mono uppercase tracking-widest text-fuchsia-300">
          <T id="waybill.salesExtract.title" />
        </h3>
        <p className="mb-2 text-xs text-slate-500">
          <T id="waybill.salesExtract.subtitle" />
        </p>
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          rows={3}
          className="w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-white placeholder-slate-500 focus:border-fuchsia-500 focus:outline-none"
        />
        <div className="mt-2 flex gap-2">
          <button
            type="button"
            onClick={onExtract}
            disabled={busy || !text.trim()}
            className="rounded-lg border border-fuchsia-500/40 bg-fuchsia-500/10 px-3 py-1.5 text-xs font-mono text-fuchsia-200 hover:bg-fuchsia-500/20 disabled:opacity-50"
          >
            {busy ? <T id="waybill.salesExtract.busy" /> : <T id="waybill.salesExtract.extract" />}
          </button>
          {draft && (
            <button
              type="button"
              onClick={() => onUse(draft)}
              className="rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-3 py-1.5 text-xs font-mono text-emerald-200 hover:bg-emerald-500/20"
            >
              <T id="waybill.salesExtract.useDraft" />
            </button>
          )}
        </div>
        {error && <div className="mt-3 rounded border border-rose-500/40 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">{error}</div>}
        {draft && (
          <div className="mt-3 space-y-2">
            <div className="text-xs text-slate-300">
              <span className="font-mono text-slate-500">
                <T id="waybill.salesExtract.customer" />
              </span> {draft.customer_name}
              {draft.customer_code && <span className="ml-2 font-mono text-cyan-300">({draft.customer_code})</span>}
              <span className="ml-2 font-mono text-slate-500">
                <T id="waybill.salesExtract.terms" />
              </span> {draft.payment_terms}
              <span className="ml-2 font-mono text-slate-500">
                <T id="waybill.salesExtract.conf" />
              </span> {Math.round((draft.confidence ?? 0) * 100)}%
            </div>
            {Array.isArray(draft.items) && draft.items.length > 0 && (
              <ul className="space-y-1 text-xs">
                {draft.items.map((it, i) => (
                  <li key={i} className="rounded border border-slate-800 bg-slate-950 px-2 py-1 font-mono text-slate-300">
                    {it.qty}× {it.description} @ {it.unit_price} = {(it.qty * it.unit_price).toLocaleString('th-TH', { minimumFractionDigits: 2 })}
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </section>

      {showRecordingSections && waybillId != null && soId != null && (
        <RecordingSections
          waybillId={waybillId}
          soId={soId}
          soItems={soItems}
          existingArSlipId={existingArSlipId}
          canRecord={canRecord}
          lang={lang}
          onRefresh={() => router.refresh()}
        />
      )}
    </>
  );
}

interface RecordingSectionsProps {
  waybillId: string;
  soId: number;
  soItems: SoItemRow[];
  existingArSlipId: number | null;
  canRecord: boolean;
  lang: 'en' | 'th' | 'de';
  onRefresh: () => void;
}

function RecordingSections({
  waybillId,
  soId,
  soItems,
  existingArSlipId,
  canRecord,
  lang,
  onRefresh,
}: RecordingSectionsProps) {
  const locale = (lang === 'de' ? 'th' : lang) as 'en' | 'th';
  return (
    <div className="mt-4 space-y-4">
      <section className="rounded-2xl border border-cyan-500/30 bg-cyan-950/15 p-4">
        <h3 className="mb-2 text-xs font-mono uppercase tracking-widest text-cyan-300">
          Customer Payment Slip
        </h3>
        <SalesPaymentSlipUpload
          waybillId={waybillId}
          soId={soId}
          locale={locale}
          existingSlipId={existingArSlipId}
          onAttached={onRefresh}
        />
      </section>

      <CoaLinesSection soId={soId} items={soItems} />

      <RecordToGlSection
        waybillId={waybillId}
        hasSlip={existingArSlipId != null}
        canRecord={canRecord}
      />
    </div>
  );
}

function CoaLinesSection({
  soId,
  items,
}: {
  soId: number;
  items: SoItemRow[];
}) {
  return (
    <section className="rounded-2xl border border-slate-700/60 bg-slate-950/40 p-4">
      <h3 className="mb-2 text-xs font-mono uppercase tracking-widest text-slate-300">
        Chart of Accounts per line
      </h3>
      {items.length === 0 ? (
        <p className="text-xs text-slate-500">No line items.</p>
      ) : (
        <ul className="space-y-2">
          {items.map((it) => (
            <li key={it.id} className="flex flex-wrap items-center gap-2 rounded-lg border border-slate-800 bg-slate-950 px-3 py-2">
              <span className="min-w-0 flex-1 text-xs text-white">{it.description}</span>
              <span className="font-mono text-xs text-slate-400">
                {it.line_total.toLocaleString('th-TH', { minimumFractionDigits: 2 })}
              </span>
              <AiCoaChip
                itemId={it.id}
                expenseId={soId}
                description={it.description}
                currentCode={it.mapped_revenue_account_code}
                currentScore={it.confidence_score}
              />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function RecordToGlSection({
  waybillId,
  hasSlip,
  canRecord,
}: {
  waybillId: string;
  hasSlip: boolean;
  canRecord: boolean;
}) {
  const disabled = !hasSlip || !canRecord;
  const label = !hasSlip
    ? 'Attach slip first'
    : !canRecord
      ? 'Recording locked'
      : 'Record to GL';
  return (
    <section className="rounded-2xl border border-emerald-500/30 bg-emerald-950/15 p-4">
      <h3 className="mb-1 text-xs font-mono uppercase tracking-widest text-emerald-300">
        Record payment
      </h3>
      <p className="mb-3 text-xs text-slate-500">
        Posts settlement journal to GL and closes this waybill.
      </p>
      <form action={recordSalesPaymentAction}>
        <input type="hidden" name="waybillId" value={waybillId} />
        <button
          type="submit"
          disabled={disabled}
          className="rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-4 py-2 text-xs font-mono font-semibold text-emerald-200 hover:bg-emerald-500/20 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {label}
        </button>
      </form>
    </section>
  );
}