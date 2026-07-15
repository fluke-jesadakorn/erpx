import Link from 'next/link';
import {
  stageLabel,
} from '@folio-lib/waybill/labels';
import { normalizeStage } from '@folio-lib/perm/stages';
import type { WaybillInboxRow, InboxScope } from '@folio-lib/waybill/queries';
import { dismissInboxItemAction } from '@/app/(app)/(protected)/inbox/_actions';

interface Props {
  scope: InboxScope;
  items: WaybillInboxRow[];
  lang?: 'en' | 'th';
}

interface OriginVisual {
  icon: string;
  label: string;
}

const ORIGIN_VISUAL: Record<string, OriginVisual> = {
  expense: { icon: '📦', label: 'Expense' },
  pr:      { icon: '📝', label: 'PR' },
  po:      { icon: '📋', label: 'PO' },
};

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

function EmptyState({ scope }: { scope: InboxScope }) {
  const text =
    scope === 'waiting'
      ? '✨ Nothing waiting on you right now'
      : scope === 'watching'
      ? '🔕 You\u2019re not watching anything'
      : '📭 Inbox is empty';
  return (
    <li className="rounded-2xl border border-slate-800 bg-slate-950/60 p-8 text-center text-sm text-slate-400">
      {text}
    </li>
  );
}

export function InboxList({ scope, items, lang: _lang = 'en' }: Props) {
  if (items.length === 0) {
    return (
      <ul role="list" className="space-y-2">
        <EmptyState scope={scope} />
      </ul>
    );
  }

  return (
    <ul role="list" className="divide-y divide-slate-800/70 rounded-2xl border border-slate-800 bg-slate-950/60">
      {items.map((it) => {
        const visual = ORIGIN_VISUAL[it.origin] ?? { icon: '📄', label: it.origin };
        const { label: stageLabelText, emoji: stageEmoji } = stageInfo(it.current_stage, it.origin);
        const amount = formatAmount(it.total_amount, it.currency);
        const age = formatAge(it.age_hours);
        const wbId: string = (it as any).waybill_id ?? (it as any).id ?? '';
        const stageKey: string = (it as any).stage_key ?? '';
        const notifiedAt: string | Date | null = (it as any).notified_at ?? null;

        const unwatched = scope === 'watching' && notifiedAt == null;
        const rowLink = `/waybill/${wbId}`;
        return (
          <li
            key={`${wbId}:${stageKey}`}
            className="flex flex-wrap items-center gap-3 px-4 py-3 transition-colors hover:bg-slate-800/40"
          >
            <Link
              href={rowLink}
              className="flex min-w-0 flex-1 items-center gap-3 outline-none focus-visible:ring-2 focus-visible:ring-cyan-500/40"
            >
              <span
                aria-hidden
                className="grid h-8 w-8 place-items-center rounded-lg border border-slate-800 bg-slate-900/60 text-base"
              >
                {visual.icon}
              </span>
              <div className="min-w-0 flex-1 space-y-0.5">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-mono text-sm text-cyan-300">
                    {it.id}
                  </span>
                  <span className="inline-flex items-center gap-1 rounded-full border border-slate-700 bg-slate-900/60 px-2 py-0.5 text-xs font-mono text-slate-300">
                    <span aria-hidden>{stageEmoji}</span>
                    <span>{stageLabelText}</span>
                  </span>
                  {unwatched && (
                    <span className="rounded-full bg-amber-500/15 px-2 py-0.5 text-xs font-mono text-amber-300">
                      NEW
                    </span>
                  )}
                </div>
                <div className="text-sm text-slate-400">
                  {visual.label} · {it.vendor_name ?? '—'}
                </div>
                <div className="text-xs font-mono text-slate-500">
                  submitter: {it.submitter_name ?? '—'}
                </div>
              </div>
            </Link>

            <div className="flex flex-col items-end gap-1 text-right">
              <span className="font-mono text-sm text-slate-200">{amount}</span>
              <span className="text-xs font-mono text-slate-500">{age}</span>
              {scope === 'waiting' && (
                <span className="rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-xs font-mono text-amber-300">
                  awaiting your action
                </span>
              )}
              {scope === 'watching' && (
                <>
                  <span className="text-xs font-mono text-slate-500">
                    🔔 since {formatNotifiedAt(notifiedAt)}
                  </span>
                  <form action={dismissInboxItemAction} className="mt-1">
                    <input type="hidden" name="waybillId" value={wbId} />
                    <input type="hidden" name="stageKey" value={stageKey} />
                    <button
                      type="submit"
                      className="rounded-md border border-slate-700 px-2 py-0.5 text-xs font-mono text-slate-400 transition-colors hover:border-rose-500/60 hover:text-rose-300"
                    >
                      Stop watching
                    </button>
                  </form>
                </>
              )}
            </div>
          </li>
        );
      })}
    </ul>
  );
}

export default InboxList;
