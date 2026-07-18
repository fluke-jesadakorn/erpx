import React from 'react';
import { PageLayout } from '@/components/PageLayout';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { getTranslations } from 'next-intl/server';

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
  const t = await getTranslations();
  return (
    <PageLayout
      title={<T id="sales.artifactsTitle" locale={locale} />}
      subtitle={t('sales.artifactsStage', { stage: currentStage })}
    >
      <section className="rounded-md border border-rule bg-paper-2/60 p-4">
        <h3 className="text-xs font-mono uppercase tracking-wider text-ink-2 mb-3">
          <T id="sales.artifactsCustomer" locale={locale} />
        </h3>
        {artifacts.customer ? (
          <a
            href={`/customers/${artifacts.customer.id}`}
            className="block rounded-md border border-info/40 bg-info-soft p-3 hover:bg-info-strong"
          >
            <div className="font-mono text-info">{artifacts.customer.code}</div>
            <div className="text-ink">{artifacts.customer.name}</div>
            {artifacts.customer.name_th ? (
              <div className="text-sm text-ink-2">{artifacts.customer.name_th}</div>
            ) : null}
          </a>
        ) : (
          <p className="text-sm text-mute">
            <T id="sales.artifactsNoCustomer" locale={locale} />
          </p>
        )}
      </section>

      <section className="mt-4 rounded-md border border-rule bg-paper-2/60 p-4">
        <h3 className="text-xs font-mono uppercase tracking-wider text-ink-2 mb-3">
          <T id="sales.artifactsLineItems" locale={locale} />
        </h3>
        {artifacts.items.length === 0 ? (
          <p className="text-sm text-mute">
            <T id="sales.artifactsNoItems" locale={locale} />
          </p>
        ) : (
          <table className="w-full text-sm">
            <thead className="border-b border-rule text-xs font-mono uppercase text-mute">
              <tr>
                <th className="px-2 py-2 text-left"><T id="sales.artifactsColDescription" locale={locale} /></th>
                <th className="px-2 py-2 text-right"><T id="sales.artifactsColQty" locale={locale} /></th>
                <th className="px-2 py-2 text-right"><T id="sales.artifactsColUnit" locale={locale} /></th>
                <th className="px-2 py-2 text-right"><T id="sales.artifactsColVat" locale={locale} /></th>
                <th className="px-2 py-2 text-right"><T id="sales.artifactsColLine" locale={locale} /></th>
              </tr>
            </thead>
            <tbody>
              {artifacts.items.map((it) => (
                <tr key={it.id} className="border-b border-rule">
                  <td className="px-2 py-2 text-ink">{it.description}</td>
                  <td className="px-2 py-2 text-right font-mono">{it.qty}</td>
                  <td className="px-2 py-2 text-right font-mono">{formatTHB(it.unit_price)}</td>
                  <td className="px-2 py-2 text-right font-mono">{formatTHB(it.vat_amount)}</td>
                  <td className="px-2 py-2 text-right font-mono text-positive">{formatTHB(it.line_total)}</td>
                </tr>
              ))}
            </tbody>
            <tfoot className="border-t border-rule text-sm font-mono">
              <tr>
                <td colSpan={4} className="px-2 py-2 text-right text-mute">
                  <T id="sales.artifactsSubtotal" locale={locale} />
                </td>
                <td className="px-2 py-2 text-right text-ink-2">{formatTHB(artifacts.totals.subtotal)}</td>
              </tr>
              <tr>
                <td colSpan={4} className="px-2 py-2 text-right text-mute">
                  <T id="sales.artifactsVat" locale={locale} />
                </td>
                <td className="px-2 py-2 text-right text-caution">{formatTHB(artifacts.totals.vat_total)}</td>
              </tr>
              <tr>
                <td colSpan={4} className="px-2 py-2 text-right text-ink font-bold">
                  <T id="sales.artifactsTotal" locale={locale} />
                </td>
                <td className="px-2 py-2 text-right text-positive font-bold">{formatTHB(artifacts.totals.total)}</td>
              </tr>
            </tfoot>
          </table>
        )}
      </section>

      <div className="mt-4 grid grid-cols-1 md:grid-cols-2 gap-4">
        <section className="rounded-md border border-rule bg-paper-2/60 p-4">
          <h3 className="text-xs font-mono uppercase tracking-wider text-ink-2 mb-2">
            <T id="sales.artifactsInvoice" locale={locale} />
          </h3>
          {artifacts.invoice?.number ? (
            <div>
              <div className="font-mono text-positive">{artifacts.invoice.number}</div>
              {artifacts.invoice.issued_at ? (
                <div className="text-sm text-ink-2">
                  <T id="sales.artifactsInvoiceIssued" locale={locale} values={{ date: artifacts.invoice.issued_at.slice(0, 10) }} />
                </div>
              ) : null}
            </div>
          ) : (
            <p className="text-sm text-mute">
              <T id="sales.artifactsInvoiceNotYet" locale={locale} values={{ stage: currentStage }} />
            </p>
          )}
        </section>

        <section className="rounded-md border border-rule bg-paper-2/60 p-4">
          <h3 className="text-xs font-mono uppercase tracking-wider text-ink-2 mb-2">
            <T id="sales.artifactsArReceipt" locale={locale} />
          </h3>
          {artifacts.ar_receipt ? (
            <a href={artifacts.ar_receipt.file_path} className="font-mono text-info hover:underline">
              {artifacts.ar_receipt.mime_type} · {artifacts.ar_receipt.uploaded_at.slice(0, 10)}
            </a>
          ) : (
            <p className="text-sm text-mute">
              <T id="sales.artifactsNoArReceipt" locale={locale} />
            </p>
          )}
        </section>
      </div>
    </PageLayout>
  );
}