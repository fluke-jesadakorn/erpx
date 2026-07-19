'use client';

import React, { useState } from 'react';
import {
  AlertOctagon,
  Bell,
  Check,
  CheckCircle2,
  Circle,
  FileText,
  PackageCheck,
  Receipt,
  ShoppingCart,
  Trash2,
  Zap,
  type LucideIcon,
} from 'lucide-react';
import { T } from '@/components/i18n/T';

export type Severity = 'info' | 'success' | 'warning' | 'error';
export type Category = 'action' | 'update';
export type Domain = 'expense' | 'so' | 'pr' | 'po' | 'other';

export interface NotificationRowItem {
  id: string;
  type: string;
  category: Category;
  domain?: Domain;
  message: string;
  messageKey?: string;
  href?: string | null;
  waybillId?: string | null;
  stageKey?: string | null;
  severity?: Severity | null;
  audience?: 'owner' | 'approver' | 'watcher' | null;
  createdAt: string;
  readAt?: string | null;
  resolvedAt?: string | null;
  resolvedBy?: string | null;
}

export type TimeBucket = 'new' | 'today' | 'yesterday' | 'thisWeek' | 'earlier';

interface Tone {
  tile: string;
  stripe: string;
  dot: string;
}

const TONE: Record<Severity, Tone> = {
  error:   { tile: 'bg-critical-soft/80 text-critical border-critical/35', stripe: 'bg-critical', dot: 'bg-critical' },
  warning: { tile: 'bg-caution-soft/80 text-caution border-caution/35',   stripe: 'bg-caution',   dot: 'bg-caution' },
  success: { tile: 'bg-positive-soft/80 text-positive border-positive/35', stripe: 'bg-positive', dot: 'bg-positive' },
  info:    { tile: 'bg-info-soft/80 text-info border-info/35',             stripe: 'bg-info',     dot: 'bg-info' },
};

function severityOf(item: NotificationRowItem): Severity {
  if (item.severity && item.severity in TONE) return item.severity;
  if (item.category === 'action' && !item.resolvedAt) return 'warning';
  if (item.resolvedAt) return 'success';
  if (item.type.includes('rejected')) return 'error';
  return 'info';
}

const ICON_BY_KEY: Record<string, LucideIcon> = {
  rejected: AlertOctagon,
  resolved: CheckCircle2,
  action: Zap,
  expense: Receipt,
  so: ShoppingCart,
  pr: FileText,
  po: PackageCheck,
  other: Bell,
};

export function iconKeyFor(item: NotificationRowItem): string {
  if (item.type.includes('rejected')) return 'rejected';
  if (item.type.includes('completed') || item.type.includes('paid') || item.resolvedAt) return 'resolved';
  if (item.category === 'action') return 'action';
  return item.domain ?? 'other';
}

export function iconForNotification(item: NotificationRowItem): LucideIcon {
  return ICON_BY_KEY[iconKeyFor(item)] ?? Bell;
}

export function severityTone(item: NotificationRowItem): Tone {
  return TONE[severityOf(item)];
}

function startOfToday(): number {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export function bucketOf(iso: string): TimeBucket {
  const t = new Date(iso).getTime();
  if (!Number.isFinite(t)) return 'earlier';
  const now = Date.now();
  const hourMs = 3_600_000;
  if (now - t < hourMs) return 'new';
  const today = startOfToday();
  if (t >= today) return 'today';
  if (t >= today - 86_400_000) return 'yesterday';
  if (t >= today - 6 * 86_400_000) return 'thisWeek';
  return 'earlier';
}

export const BUCKET_ORDER: TimeBucket[] = ['new', 'today', 'yesterday', 'thisWeek', 'earlier'];

export function relTime(iso: string): string {
  const seconds = Math.max(0, Math.floor((Date.now() - new Date(iso).getTime()) / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;
  const weeks = Math.floor(days / 7);
  if (weeks < 5) return `${weeks}w`;
  const months = Math.floor(days / 30);
  return `${months}mo`;
}

interface RowProps {
  item: NotificationRowItem;
  onOpen?: (item: NotificationRowItem) => void;
  onToggleRead?: (item: NotificationRowItem) => void;
  onDelete?: (item: NotificationRowItem) => void;
  compact?: boolean;
  alwaysShowActions?: boolean;
}

export const NotificationItemRow: React.FC<RowProps> = ({
  item,
  onOpen,
  onToggleRead,
  onDelete,
  compact = false,
  alwaysShowActions = false,
}) => {
  const [hover, setHover] = useState(false);
  const unread = !item.readAt;
  const tone = severityTone(item);
  const IconCmp = ICON_BY_KEY[iconKeyFor(item)] ?? Bell;
  const isAction = item.category === 'action' && !item.resolvedAt;
  const deletable = item.category === 'update' || !!item.resolvedAt;

  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      onFocus={() => setHover(true)}
      onBlur={() => setHover(false)}
      className={[
        'group relative flex items-start gap-3 rounded-xl border px-3 py-3 transition-all duration-200',
        compact ? 'min-h-[68px]' : 'min-h-[76px]',
        unread
          ? 'border-accent/40 bg-accent-soft/25 hover:border-accent/60 hover:bg-accent-soft/40'
          : 'border-rule bg-paper-2/40 hover:border-rule-strong hover:bg-paper-2/70',
      ].join(' ')}
    >
      <span
        aria-hidden
        className={[
          'pointer-events-none absolute left-0 top-3 bottom-3 w-[3px] rounded-r-full transition-opacity',
          unread ? 'opacity-100' : 'opacity-0',
          tone.stripe,
        ].join(' ')}
      />

      <span
        aria-hidden
        className={[
          'grid h-9 w-9 shrink-0 place-items-center rounded-lg border',
          tone.tile,
        ].join(' ')}
      >
        <IconCmp size={16} strokeWidth={2} />
      </span>

      <div className="min-w-0 flex-1">
        <button
          type="button"
          onClick={() => onOpen?.(item)}
          className="block w-full text-left"
        >
          <span className={`block break-words text-sm leading-snug ${unread ? 'font-semibold text-ink' : 'font-medium text-ink-2'}`}>
            {item.message}
          </span>
        </button>

        <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] font-mono">
          <span className={unread ? 'text-accent' : 'text-mute'} suppressHydrationWarning>{relTime(item.createdAt)}</span>
          {item.waybillId && (
            <span className="rounded-md border border-rule bg-paper-2/60 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-ink-2">
              {item.waybillId}
            </span>
          )}
          {item.stageKey && (
            <span className="rounded-md border border-rule bg-paper-2/60 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-ink-2">
              {item.stageKey.replace(/_/g, ' ')}
            </span>
          )}
          {isAction && (
            <span className="inline-flex items-center gap-1 rounded-md border border-caution/35 bg-caution-soft/70 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-caution">
              <Zap size={9} aria-hidden /> <T id="ai.notification.actionBadge" hideSecondary />
            </span>
          )}
          {item.resolvedAt && (
            <span className="inline-flex items-center gap-1 rounded-md border border-positive/35 bg-positive-soft/70 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-positive">
              <CheckCircle2 size={9} aria-hidden /> <T id="ai.notification.resolvedBadge" hideSecondary />
              {item.resolvedBy ? ` · ${item.resolvedBy}` : ''}
            </span>
          )}
        </div>
      </div>

      <div
        className={[
          'flex shrink-0 items-center gap-0.5 self-center',
          alwaysShowActions || hover ? 'opacity-100' : 'opacity-0',
          'transition-opacity duration-150',
        ].join(' ')}
      >
        {onToggleRead && (
          <button
            type="button"
            onClick={() => onToggleRead(item)}
            aria-label={unread ? 'Mark as read' : 'Mark as unread'}
            className={[
              'inline-flex h-7 w-7 items-center justify-center rounded-md transition-colors',
              unread
                ? 'text-accent hover:bg-accent-soft/60 hover:text-accent-strong'
                : 'text-mute hover:bg-paper-3/70 hover:text-ink',
            ].join(' ')}
          >
            {unread ? <Check size={14} aria-hidden /> : <Circle size={14} aria-hidden />}
          </button>
        )}
        {onDelete && deletable && (
          <button
            type="button"
            onClick={() => onDelete(item)}
            aria-label="Delete notification"
            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-mute transition-colors hover:bg-critical-soft/60 hover:text-critical"
          >
            <Trash2 size={14} aria-hidden />
          </button>
        )}
      </div>
    </div>
  );
};

export default NotificationItemRow;
