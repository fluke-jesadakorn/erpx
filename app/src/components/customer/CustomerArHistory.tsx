import React from 'react';
import type { SecondaryLocale } from '@folio-lib/server/locale';
import type { BilingualText } from '@folio-lib/i18n/types';
import { Bilingual } from '@/components/i18n/Bilingual';
import custDict from '@folio-lib/i18n/customers';

export interface CustomerArBucket {
  bucket: string;
  days_from: number;
  days_to: number;
  amount_thb: number;
  so_count: number;
}

export interface CustomerArHistoryProps {
  data: CustomerArBucket[] | null;
  totalInvoiced: number;
  totalPaid: number;
  creditLimit: number;
  customerName?: string | null;
  locale?: SecondaryLocale;
}

function formatTHB(n: number, locale: SecondaryLocale): string {
  return (Number(n) || 0).toLocaleString(locale === 'de' ? 'de-DE' : 'th-TH', { maximumFractionDigits: 0 });
}

function pickText(key: string): BilingualText {
  const en = custDict.en[key] ?? key;
  return { en, th: custDict.th?.[key], de: custDict.de?.[key] };
}

const BUCKET_KEY: Record<string, string> = {
  '0-30': 'customers.ar.bucket.0_30',
  '31-60': 'customers.ar.bucket.31_60',
  '61-90': 'customers.ar.bucket.61_90',
  '90+': 'customers.ar.bucket.90_plus',
};
const BUCKET_TONE: Record<string, string> = {
  '0-30': 'bg-emerald-500',
  '31-60': 'bg-cyan-500',
  '61-90': 'bg-amber-500',
  '90+': 'bg-rose-500',
};
const BUCKET_TONE_BG: Record<string, string> = {
  '0-30': 'bg-emerald-500/15 border-emerald-500/30 text-emerald-200',
  '31-60': 'bg-cyan-500/15 border-cyan-500/30 text-cyan-200',
  '61-90': 'bg-amber-500/15 border-amber-500/30 text-amber-200',
  '90+': 'bg-rose-500/15 border-rose-500/30 text-rose-200',
};
const ALL_BUCKETS = ['0-30', '31-60', '61-90', '90+'];

export function CustomerArHistory({
  data,
  totalInvoiced,
  totalPaid,
  creditLimit,
  customerName,
  locale = 'th',
}: CustomerArHistoryProps): React.JSX.Element | null {
  if (!data) return null;

  const bucketsByKey = new Map<string, CustomerArBucket>();
  for (const b of data) bucketsByKey.set(b.bucket, b);

  const outstanding = Math.max(0, totalInvoiced - totalPaid);
  const utilizationPct =
    creditLimit > 0 ? Math.min(100, Math.round((outstanding / creditLimit) * 100)) : 0;
  const utilizationTone =
    utilizationPct >= 100
      ? 'text-rose-300'
      : utilizationPct >= 80
      ? 'text-amber-300'
      : 'text-emerald-300';

  const titleText = pickText('customers.ar.title');
  const headingText = pickText('customers.ar.heading');
  const invoicedText = pickText('customers.ar.invoiced');
  const paidText = pickText('customers.ar.paid');
  const outstandingText = pickText('customers.ar.outstanding');
  const utilizationText = pickText('customers.ar.utilization');

  return (
    <section
      aria-label={titleText.en}
      className="rounded-2xl border border-slate-700/70 bg-slate-950/45 p-4 shadow-inner"
    >
      <header className="mb-3 flex items-center justify-between gap-2">
        <div className="text-xs font-mono uppercase tracking-widest text-slate-400">
          <Bilingual en={headingText.en} th={headingText.th} de={headingText.de} locale={locale} />
          {customerName && (
            <span className="ml-2 text-slate-300">· {customerName}</span>
          )}
        </div>
        <div className="flex items-center gap-2 text-xs font-mono text-slate-500">
          <span>
            <Bilingual en={invoicedText.en} th={invoicedText.th} de={invoicedText.de} locale={locale} />
          </span>
          <span className="text-slate-200 tabular-nums">{formatTHB(totalInvoiced, locale)}</span>
          <span className="text-slate-700">·</span>
          <span>
            <Bilingual en={paidText.en} th={paidText.th} de={paidText.de} locale={locale} />
          </span>
          <span className="text-emerald-300 tabular-nums">{formatTHB(totalPaid, locale)}</span>
        </div>
      </header>

      <ul className="space-y-1.5">
        {ALL_BUCKETS.map((key) => {
          const b = bucketsByKey.get(key) ?? null;
          const amount = b?.amount_thb ?? 0;
          const soCount = b?.so_count ?? 0;
          const max = Math.max(
            1,
            ...Array.from(bucketsByKey.values()).map((x) => x.amount_thb),
          );
          const widthPct = b ? Math.max(2, Math.round((amount / max) * 100)) : 0;
          const bucketDict = BUCKET_KEY[key] ? pickText(BUCKET_KEY[key]) : null;
          const soKey = soCount === 0
            ? 'customers.ar.so_count_0'
            : soCount === 1
            ? 'customers.ar.so_count_1'
            : 'customers.ar.so_count';
          const soDict = pickText(soKey);
          return (
            <li
              key={key}
              className={[
                'flex items-center gap-3 rounded-xl border px-3 py-2',
                b
                  ? BUCKET_TONE_BG[key] ?? 'border-slate-700 bg-slate-900/40 text-slate-300'
                  : 'border-slate-800 bg-slate-900/30 text-slate-500',
              ].join(' ')}
            >
              <div className="w-20 text-xs font-mono uppercase tracking-wider shrink-0">
                {bucketDict ? (
                  <Bilingual en={bucketDict.en} th={bucketDict.th} de={bucketDict.de} locale={locale} />
                ) : (
                  `${b?.days_from ?? 0}-${b?.days_to === 9999 ? '+' : (b?.days_to ?? 0)}`
                )}
              </div>
              <div className="relative flex-1 h-2 overflow-hidden rounded-full bg-slate-900/60">
                {b && (
                  <div
                    className={['h-full rounded-full transition-[width] duration-500', BUCKET_TONE[key] ?? 'bg-slate-500'].join(' ')}
                    style={{ width: `${widthPct}%` }}
                    aria-hidden
                  />
                )}
              </div>
              <div className="w-32 shrink-0 text-right">
                <div className="text-sm font-mono font-bold tabular-nums">
                  {formatTHB(amount, locale)} THB
                </div>
                <div className="text-xs font-mono text-slate-500">
                  <Bilingual
                    en={soDict.en.replace('{n}', String(soCount)).replace('{plural}', soCount === 1 ? '' : 's')}
                    th={soDict.th?.replace('{n}', String(soCount)).replace('{plural}', soCount === 1 ? '' : 's')}
                    de={soDict.de?.replace('{n}', String(soCount)).replace('{plural}', soCount === 1 ? '' : 's')}
                    locale={locale}
                  />
                </div>
              </div>
            </li>
          );
        })}
      </ul>

      <footer className="mt-3 grid grid-cols-2 gap-2 text-xs font-mono">
        <div className="rounded-xl border border-slate-800 bg-slate-950/50 px-3 py-2">
          <div className="text-slate-500 uppercase tracking-wider">
            <Bilingual en={outstandingText.en} th={outstandingText.th} de={outstandingText.de} locale={locale} />
          </div>
          <div className="text-base font-bold text-amber-200 tabular-nums">
            {formatTHB(outstanding, locale)} THB
          </div>
        </div>
        <div className="rounded-xl border border-slate-800 bg-slate-950/50 px-3 py-2">
          <div className="text-slate-500 uppercase tracking-wider">
            <Bilingual en={utilizationText.en} th={utilizationText.th} de={utilizationText.de} locale={locale} />
          </div>
          <div className={['text-base font-bold tabular-nums', utilizationTone].join(' ')}>
            <Bilingual
              en={`${utilizationPct}% · ${formatTHB(creditLimit, locale)}`}
              th={`${utilizationPct}% · ${formatTHB(creditLimit, locale)}`}
              de={`${utilizationPct}% · ${formatTHB(creditLimit, locale)}`}
              locale={locale}
            />
          </div>
        </div>
      </footer>
    </section>
  );
}

export default CustomerArHistory;
