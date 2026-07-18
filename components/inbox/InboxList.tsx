import Link from 'next/link';
import {
  stageLabel,
} from '@/waybill/labels';
import { normalizeStage } from '@/perm/stages';
import type { WaybillInboxRow, InboxScope } from '@/waybill/queries';
import { dismissInboxItemAction } from '@/app/(app)/(protected)/inbox/_actions';
import { T } from '@/components/i18n/T';
import { ListRow } from '@/components/ui/ListRow';
import { Empty } from '@/components/ui/Empty';
import { Bell, FileText, PackageCheck, ReceiptText, X } from 'lucide-react';

interface Props {
  scope: InboxScope;
  items: WaybillInboxRow[];
  lang?: 'en' | 'th';
}

interface OriginVisual {
  icon: 'expense' | 'pr' | 'po';
  label: string;
}

const ORIGIN_VISUAL: Record<string, OriginVisual> = {
  expense: { icon: 'expense', label: 'waybill.originExpense' },
  pr:      { icon: 'pr', label: 'waybill.originPr' },
  po:      { icon: 'po', label: 'waybill.originPo' },
};

function OriginIcon({ kind }: { kind: OriginVisual['icon'] | 'other' }) {
  if (kind === 'expense') return <ReceiptText size={16} aria-hidden />;
  if (kind === 'pr') return <FileText size={16} aria-hidden />;
  if (kind === 'po') return <PackageCheck size={16} aria-hidden />;
  return <FileText size={16} aria-hidden />;
}

function formatAge(hours: number): string {
  const h = Math.max(0, Math.floor(hours));
  if (h < 1) return '<1h';
  if (h < 24) return `${h}h`;
  if (h < 24 * 30) return `${Math.floor(h / 24)}d`;
  return `${Math.floor(h / (24 * 30))}mo`;
}

function formatAmount(value: string | null, currency: string): string {
  if (value == null) return '—';
  const n = parseFloat(value);
  if (!Number.isFinite(n)) return '—';
  return `${n.toLocaleString('th-TH', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ${currency}`;
}

function formatNotifiedAt(date: Date | string | null | undefined): string {
  if (!date) return '';
  const d = new Date(date);
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${yyyy}-${mm}-${dd}`;
}

function stageInfo(currentStage: string, origin: string) {
  const key = normalizeStage(currentStage) ?? currentStage;
  const domain = origin === 'expense' ? 'expense' : 'procurement';
  return { key, domain, ...stageLabel(key, domain) };
}

export function InboxList({ scope, items, lang: _lang = 'en' }: Props) {
  if (items.length === 0) {
    if (scope === 'waiting') {
      return <Empty title="All clear" body="Nothing waiting on you right now." />;
    }
    if (scope === 'watching') {
      return <Empty title="No watched items" body="You\u2019re not watching anything." />;
    }
    return <Empty title="Inbox is empty" body="No items to show here." />;
  }

  return (
    <ul role="list" className="panel divide-y divide-rule overflow-hidden">
      {items.map((it) => {
        const visual = ORIGIN_VISUAL[it.origin] ?? { icon: 'other' as const, label: it.origin };
        const { label: stageLabelText } = stageInfo(it.current_stage, it.origin);
        const amount = formatAmount(it.total_amount, it.currency);
        const age = formatAge(it.age_hours);
        const wbId: string = (it as any).waybill_id ?? (it as any).id ?? '';
        const stageKey: string = (it as any).stage_key ?? '';
        const notifiedAt: string | Date | null = (it as any).notified_at ?? null;

        const unwatched = scope === 'watching' && notifiedAt == null;
        const rowLink = `/waybill/${wbId}`;
        return (
          <ListRow
            key={`${wbId}:${stageKey}`}
            className="flex-wrap px-4 py-3"
          >
            <Link
              href={rowLink}
              className="flex min-w-0 flex-1 items-center gap-3 outline-none focus-visible:ring-2 focus-visible:ring-info/40"
            >
              <span
                aria-hidden
                className="grid h-8 w-8 place-items-center rounded-lg border border-rule bg-paper-2/60 text-base"
              >
                <OriginIcon kind={visual.icon} />
              </span>
              <div className="min-w-0 flex-1 space-y-0.5">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-mono text-sm text-info">
                    {it.id}
                  </span>
                  <span className="inline-flex items-center gap-1 rounded-full border border-rule bg-paper-2/60 px-2 py-0.5 text-xs font-mono text-ink-2">
                    <T id={stageLabelText} />
                  </span>
                  {unwatched && (
                    <span className="rounded-full bg-caution-soft px-2 py-0.5 text-xs font-mono text-caution">
                      NEW
                    </span>
                  )}
                </div>
                <div className="text-sm text-ink-2">
                   <T id={visual.label} /> · {it.vendor_name ?? '—'}
                </div>
                <div className="text-xs font-mono text-mute">
                  submitter: {it.submitter_name ?? '—'}
                </div>
              </div>
            </Link>

            <div className="flex flex-col items-end gap-1 text-right">
              <span className="font-mono text-sm text-ink">{amount}</span>
              <span className="text-xs font-mono text-mute">{age}</span>
              {scope === 'waiting' && (
                <span className="rounded-md border border-caution/30 bg-caution-soft px-2 py-0.5 text-xs font-mono text-caution">
                  awaiting your action
                </span>
              )}
              {scope === 'watching' && (
                <>
                  <span className="text-xs font-mono text-mute">
                    <Bell size={11} className="mr-1 inline" aria-hidden /> since {formatNotifiedAt(notifiedAt)}
                  </span>
                  <form action={dismissInboxItemAction} className="mt-1">
                    <input type="hidden" name="waybillId" value={wbId} />
                    <input type="hidden" name="stageKey" value={stageKey} />
                    <button
                      type="submit"
                      className="rounded-md border border-rule px-2 py-0.5 text-xs font-mono text-ink-2 transition-colors hover:border-critical/60 hover:text-critical"
                    >
                      <X size={11} className="mr-1 inline" aria-hidden /> Stop watching
                    </button>
                  </form>
                </>
              )}
            </div>
          </ListRow>
        );
      })}
    </ul>
  );
}

export default InboxList;
