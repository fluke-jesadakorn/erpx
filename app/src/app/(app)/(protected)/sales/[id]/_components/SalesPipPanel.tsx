import React from 'react';
import { PageLayout } from '@/components/PageLayout';
import { getTextServer } from '@/components/i18n/server';
import { Bilingual } from '@/components/i18n/Bilingual';
import { intlLocale } from '@/components/i18n/sec';
import { getSecondaryLocale, type SecondaryLocale } from '@folio-lib/server/locale';
import salesDict from '@folio-lib/i18n/sales';

export interface SalesArtifactsLite {
  customer: { id: number; code: string; name: string; name_th?: string | null } | null;
  items: Array<{ id: number; description: string; qty: number; unit_price: number; vat_amount: number; line_total: number }>;
  totals: { subtotal: number; vat_total: number; total: number };
  invoice: { number: string | null; issued_at: string | null } | null;
  ar_receipt: { file_path: string; mime_type: string; uploaded_at: string } | null;
}

function formatTHB(n: number): string {
  return (Number(n) || 0).toLocaleString('th-TH', { minimumFractionDigits: 2 });
}

function fmtDate(iso: string, locale: SecondaryLocale): string {
  return new Intl.DateTimeFormat(intlLocale(locale), {
    day: '2-digit',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  }).format(new Date(iso));
}

function fill(tmpl: string, vars: Record<string, string | number>): string {
  return tmpl.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`));
}

function pickLocale(dict: { en: string; th?: string; de?: string }, locale: SecondaryLocale): string {
  return dict[locale] ?? dict.en;
}

export async function SalesPipPanel({
  waybillId,
  artifacts,
  currentStage,
}: {
  waybillId: string;
  artifacts: SalesArtifactsLite;
  currentStage: string;
}) {
  void waybillId;
  const locale = await getSecondaryLocale();
  const title = getTextServer(salesDict, 'sales.artifacts.title', locale);
  const stageText = getTextServer(salesDict, 'sales.artifacts.stage', locale);
  const customerText = getTextServer(salesDict, 'sales.artifacts.customer', locale);
  const noCustomer = getTextServer(salesDict, 'sales.artifacts.no_customer', locale);
  const lineItemsText = getTextServer(salesDict, 'sales.artifacts.line_items', locale);
  const noItems = getTextServer(salesDict, 'sales.artifacts.no_items', locale);
  const colDescription = getTextServer(salesDict, 'sales.artifacts.col.description', locale);
  const colQty = getTextServer(salesDict, 'sales.artifacts.col.qty', locale);
  const colUnit = getTextServer(salesDict, 'sales.artifacts.col.unit', locale);
  const colVat = getTextServer(salesDict, 'sales.artifacts.col.vat', locale);
  const colLine = getTextServer(salesDict, 'sales.artifacts.col.line', locale);
  const subtotal = getTextServer(salesDict, 'sales.artifacts.subtotal', locale);
  const vat = getTextServer(salesDict, 'sales.artifacts.vat', locale);
  const total = getTextServer(salesDict, 'sales.artifacts.total', locale);
  const invoiceText = getTextServer(salesDict, 'sales.artifacts.invoice', locale);
  const invoiceIssued = getTextServer(salesDict, 'sales.artifacts.invoice.issued', locale);
  const invoiceNotYet = getTextServer(salesDict, 'sales.artifacts.invoice.not_yet', locale);
  const arReceiptText = getTextServer(salesDict, 'sales.artifacts.ar_receipt', locale);
  const noArReceipt = getTextServer(salesDict, 'sales.artifacts.no_ar_receipt', locale);
  return (
    <PageLayout
      title={<Bilingual en={title.en} th={title.th} de={title.de} />}
      subtitle={fill(pickLocale(stageText, locale), { stage: currentStage })}
    >
      <section className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4">
        <h3 className="text-xs font-mono uppercase tracking-wider text-slate-400 mb-3">
          <Bilingual en={customerText.en} th={customerText.th} de={customerText.de} />
        </h3>
        {artifacts.customer ? (
          <a
            href={`/customers/${artifacts.customer.id}`}
            className="block rounded-xl border border-cyan-500/30 bg-cyan-950/20 p-3 hover:bg-cyan-950/40"
          >
            <div className="font-mono text-cyan-300">{artifacts.customer.code}</div>
            <div className="text-white">{artifacts.customer.name}</div>
            {artifacts.customer.name_th ? (
              <div className="text-sm text-slate-400">{artifacts.customer.name_th}</div>
            ) : null}
          </a>
        ) : (
          <p className="text-sm text-slate-500">
            <Bilingual en={noCustomer.en} th={noCustomer.th} de={noCustomer.de} />
          </p>
        )}
      </section>

      <section className="mt-4 rounded-2xl border border-slate-800 bg-slate-950/60 p-4">
        <h3 className="text-xs font-mono uppercase tracking-wider text-slate-400 mb-3">
          <Bilingual en={lineItemsText.en} th={lineItemsText.th} de={lineItemsText.de} />
        </h3>
        {artifacts.items.length === 0 ? (
          <p className="text-sm text-slate-500">
            <Bilingual en={noItems.en} th={noItems.th} de={noItems.de} />
          </p>
        ) : (
          <table className="w-full text-sm">
            <thead className="border-b border-slate-800 text-xs font-mono uppercase text-slate-500">
              <tr>
                <th className="px-2 py-2 text-left">
                  <Bilingual en={colDescription.en} th={colDescription.th} de={colDescription.de} />
                </th>
                <th className="px-2 py-2 text-right">
                  <Bilingual en={colQty.en} th={colQty.th} de={colQty.de} />
                </th>
                <th className="px-2 py-2 text-right">
                  <Bilingual en={colUnit.en} th={colUnit.th} de={colUnit.de} />
                </th>
                <th className="px-2 py-2 text-right">
                  <Bilingual en={colVat.en} th={colVat.th} de={colVat.de} />
                </th>
                <th className="px-2 py-2 text-right">
                  <Bilingual en={colLine.en} th={colLine.th} de={colLine.de} />
                </th>
              </tr>
            </thead>
            <tbody>
              {artifacts.items.map((it) => (
                <tr key={it.id} className="border-b border-slate-900">
                  <td className="px-2 py-2 text-white">{it.description}</td>
                  <td className="px-2 py-2 text-right font-mono">{it.qty}</td>
                  <td className="px-2 py-2 text-right font-mono">{formatTHB(it.unit_price)}</td>
                  <td className="px-2 py-2 text-right font-mono">{formatTHB(it.vat_amount)}</td>
                  <td className="px-2 py-2 text-right font-mono text-emerald-300">{formatTHB(it.line_total)}</td>
                </tr>
              ))}
            </tbody>
            <tfoot className="border-t border-slate-800 text-sm font-mono">
              <tr>
                <td colSpan={4} className="px-2 py-2 text-right text-slate-500">
                  <Bilingual en={subtotal.en} th={subtotal.th} de={subtotal.de} />
                </td>
                <td className="px-2 py-2 text-right text-slate-300">{formatTHB(artifacts.totals.subtotal)}</td>
              </tr>
              <tr>
                <td colSpan={4} className="px-2 py-2 text-right text-slate-500">
                  <Bilingual en={vat.en} th={vat.th} de={vat.de} />
                </td>
                <td className="px-2 py-2 text-right text-amber-300">{formatTHB(artifacts.totals.vat_total)}</td>
              </tr>
              <tr>
                <td colSpan={4} className="px-2 py-2 text-right text-white font-bold">
                  <Bilingual en={total.en} th={total.th} de={total.de} />
                </td>
                <td className="px-2 py-2 text-right text-emerald-300 font-bold">{formatTHB(artifacts.totals.total)}</td>
              </tr>
            </tfoot>
          </table>
        )}
      </section>

      <div className="mt-4 grid grid-cols-1 md:grid-cols-2 gap-4">
        <section className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4">
          <h3 className="text-xs font-mono uppercase tracking-wider text-slate-400 mb-2">
            <Bilingual en={invoiceText.en} th={invoiceText.th} de={invoiceText.de} />
          </h3>
          {artifacts.invoice?.number ? (
            <div>
              <div className="font-mono text-emerald-300">{artifacts.invoice.number}</div>
              {artifacts.invoice.issued_at ? (
                <div className="text-sm text-slate-400">
                  {fill(pickLocale(invoiceIssued, locale), {
                    date: fmtDate(artifacts.invoice.issued_at, locale),
                  })}
                </div>
              ) : null}
            </div>
          ) : (
            <p className="text-sm text-slate-500">
              {fill(pickLocale(invoiceNotYet, locale), { stage: currentStage })}
            </p>
          )}
        </section>

        <section className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4">
          <h3 className="text-xs font-mono uppercase tracking-wider text-slate-400 mb-2">
            <Bilingual en={arReceiptText.en} th={arReceiptText.th} de={arReceiptText.de} />
          </h3>
          {artifacts.ar_receipt ? (
            <a href={artifacts.ar_receipt.file_path} className="font-mono text-cyan-300 hover:underline">
              {artifacts.ar_receipt.mime_type} · {fmtDate(artifacts.ar_receipt.uploaded_at, locale)}
            </a>
          ) : (
            <p className="text-sm text-slate-500">
              <Bilingual en={noArReceipt.en} th={noArReceipt.th} de={noArReceipt.de} />
            </p>
          )}
        </section>
      </div>
    </PageLayout>
  );
}
