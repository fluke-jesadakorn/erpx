import React from 'react';
import { T } from '@/components/i18n/T';

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
}

function formatTHB(n: number): string {
  return (Number(n) || 0).toLocaleString('th-TH', { maximumFractionDigits: 0 });
}

const BUCKET_KEY: Record<string, string> = {
  '0-30': 'customers.arBucket0to30',
  '31-60': 'customers.arBucket31to60',
  '61-90': 'customers.arBucket61to90',
  '90+': 'customers.arBucket90Plus',
};
const BUCKET_TONE: Record<string, string> = {
  '0-30': 'bg-positive',
  '31-60': 'bg-info',
  '61-90': 'bg-caution',
  '90+': 'bg-critical',
};
const BUCKET_TONE_BG: Record<string, string> = {
  '0-30': 'bg-positive-soft border-positive/40 text-positive-strong',
  '31-60': 'bg-info-soft border-info/40 text-info-strong',
  '61-90': 'bg-caution-soft border-caution/40 text-caution-strong',
  '90+': 'bg-critical-soft border-critical/40 text-critical-strong',
};
const ALL_BUCKETS = ['0-30', '31-60', '61-90', '90+'];

export function CustomerArHistory({
  data,
  totalInvoiced,
  totalPaid,
  creditLimit,
  customerName,
}: CustomerArHistoryProps): React.JSX.Element | null {
  if (!data) return null;

  const bucketsByKey = new Map<string, CustomerArBucket>();
  for (const b of data) bucketsByKey.set(b.bucket, b);

  const outstanding = Math.max(0, totalInvoiced - totalPaid);
  const utilizationPct =
    creditLimit > 0 ? Math.min(100, Math.round((outstanding / creditLimit) * 100)) : 0;
  const utilizationTone =
    utilizationPct >= 100
      ? 'text-critical'
      : utilizationPct >= 80
      ? 'text-caution'
      : 'text-positive';

  return (
    <section
      aria-label="AR history"
      className="rounded-md border border-rule bg-paper-3/60 p-4"
    >
      <header className="mb-3 flex items-center justify-between gap-2">
        <div className="text-xs font-mono uppercase tracking-widest text-ink-2">
          <T id="customers.arHeading" />
          {customerName && (
            <span className="ml-2 text-ink">· {customerName}</span>
          )}
        </div>
        <div className="flex items-center gap-2 text-xs font-mono text-ink-2">
          <span>
            <T id="customers.arInvoiced" />
          </span>
          <span className="text-ink tabular-nums">{formatTHB(totalInvoiced)}</span>
          <span className="text-mute">·</span>
          <span>
            <T id="customers.arPaid" />
          </span>
          <span className="text-positive tabular-nums">{formatTHB(totalPaid)}</span>
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
          return (
            <li
              key={key}
              className={[
                'flex items-center gap-3 rounded-md border px-3 py-2',
                b
                  ? BUCKET_TONE_BG[key] ?? 'border-rule bg-paper-3 text-ink-2'
                  : 'border-rule bg-paper-2 text-mute',
              ].join(' ')}
            >
              <div className="w-20 text-xs font-mono uppercase tracking-wider shrink-0">
                <T id={BUCKET_KEY[key]} />
              </div>
              <div className="relative flex-1 h-2 overflow-hidden rounded-full bg-paper">
                {b && (
                  <div
                    className={['h-full rounded-full transition-[width] duration-500', BUCKET_TONE[key] ?? 'bg-mute'].join(' ')}
                    style={{ width: `${widthPct}%` }}
                    aria-hidden
                  />
                )}
              </div>
              <div className="w-32 shrink-0 text-right">
                <div className="text-sm font-mono font-bold tabular-nums text-ink">
                  {formatTHB(amount)} THB
                </div>
                <div className="text-xs font-mono text-ink-2">
                  {soCount === 0 ? (
                    <T id="customers.arSoCountZero" />
                  ) : soCount === 1 ? (
                    <T id="customers.arSoCountOne" />
                  ) : (
                    <T id="customers.arSoCount" values={{ n: soCount }} />
                  )}
                </div>
              </div>
            </li>
          );
        })}
      </ul>

      <footer className="mt-3 grid grid-cols-2 gap-2 text-xs font-mono">
        <div className="rounded-md border border-rule bg-paper-3 px-3 py-2">
          <div className="text-ink-2 uppercase tracking-wider">
            <T id="customers.arOutstanding" />
          </div>
          <div className="text-base font-bold text-caution tabular-nums">
            {formatTHB(outstanding)} THB
          </div>
        </div>
        <div className="rounded-md border border-rule bg-paper-3 px-3 py-2">
          <div className="text-ink-2 uppercase tracking-wider">
            <T id="customers.arUtilization" />
          </div>
          <div className={['text-base font-bold tabular-nums', utilizationTone].join(' ')}>
            {utilizationPct}% · {formatTHB(creditLimit)}
          </div>
        </div>
      </footer>
    </section>
  );
}

export default CustomerArHistory;