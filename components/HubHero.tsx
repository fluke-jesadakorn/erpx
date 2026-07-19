'use client';

import React from 'react';
import Link from 'next/link';
import { HubAmbient } from './HubAmbient';
import { TileTooltipProvider } from './TileTooltip';
import { type TileWithMeta } from './tile-config';
import {
  roleLabel,
  roleBadge,
  roleGlyph,
  staffLevelLabel,
  staffLevelGlyph,
} from './UserAvatar';
import {
  greetingLine,
  kpiSummary,
  timeGreeting,
  type GreetingKey,
} from '@/hero';
import type { ActionQueueSummary } from '@/notifications/queries';
import { T } from '@/components/i18n/T';
import { useFormatMoney } from '@/components/i18n/formatters';
import {
  Bell,
  CheckCircle2,
  CloudSun,
  MoonStar,
  Search,
  ShoppingCart,
  Sun,
  UserRound,
  type LucideIcon,
} from 'lucide-react';

interface HubHeroProps {
  actor: {
    id?: number;
    fullname?: string | null;
    role_name?: string | null;
    department?: string | null;
    dept_group_name?: string | null;
    employee_code?: string | null;
    level?: number | null;
  } | null;
  tiles: TileWithMeta[];
  actionQueue: ActionQueueSummary;
  isLocked: (t: TileWithMeta) => boolean;
  onOpenCommand: () => void;
  onOpenPersona?: () => void;
  onOpenNotifications?: () => void;
  unreadCount?: number;
  initialGreetingKey?: GreetingKey;
}

const QUICK_ACTIONS: Array<{
  key: 'search' | 'persona' | 'notifications';
  id: string;
  hintId: string;
  icon: LucideIcon;
  tone: 'accent' | 'info' | 'caution';
}> = [
  { key: 'search',        id: 'hub.quickSearchLabel',        hintId: 'hub.quickSearchHint',        icon: Search,    tone: 'accent' },
  { key: 'persona',       id: 'hub.quickPersonaLabel',       hintId: 'hub.quickPersonaHint',       icon: UserRound, tone: 'info' },
  { key: 'notifications', id: 'hub.quickNotificationsLabel', hintId: 'hub.quickNotificationsHint', icon: Bell,      tone: 'caution' },
];

export const HubHero: React.FC<HubHeroProps> = ({
  actor,
  tiles,
  actionQueue,
  isLocked,
  onOpenCommand,
  onOpenPersona,
  onOpenNotifications,
  unreadCount,
  initialGreetingKey,
}) => {
  const kpis = kpiSummary(tiles, isLocked);
  const fmtMoney = useFormatMoney();

  const fullname = (actor?.fullname || '').trim() || 'there';
  const greetingKey: GreetingKey = greetingLine(fullname, initialGreetingKey ?? timeGreeting());
  const greetId = `hero.greeting.${greetingKey}`;
  const role = actor?.role_name ?? undefined;
  const dept = actor?.dept_group_name || actor?.department || null;
  const level = actor?.level ?? null;
  const levelLabel = level != null ? staffLevelLabel(level) : null;
  const levelGlyph = level != null ? staffLevelGlyph(level) : '·';

  const GreetingIcon = greetingKey === 'morning' ? Sun : greetingKey === 'afternoon' ? CloudSun : MoonStar;

  return (
    <TileTooltipProvider>
      <section
        aria-label="Hub hero"
        className="panel-elevated relative overflow-hidden animate-fade-in"
      >
        <HubAmbient variant="hero" />

        <div className="relative z-10 p-5 sm:p-7 lg:p-8">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex flex-wrap items-center gap-2">
              <span className="inline-flex items-center gap-2 rounded-full border border-accent/35 bg-accent-soft/55 px-3 py-1.5 text-[11px] font-semibold uppercase tracking-[0.14em] text-accent">
                <span className="h-1.5 w-1.5 rounded-full bg-accent shadow-[0_0_8px_currentcolor] animate-pulse" />
                <T id="hub.live" hideSecondary />
              </span>
              {role && (
                <span className={`inline-flex items-center gap-1.5 rounded-full border px-3 py-1.5 text-[11px] font-semibold uppercase tracking-[0.12em] ${roleBadge(role)}`}>
                  <span aria-hidden>{roleGlyph(role)}</span>
                  <span>{roleLabel(role)}</span>
                </span>
              )}
              {level != null && levelLabel && (
                <span className="inline-flex items-center gap-1.5 rounded-full border border-rule bg-paper-2/45 px-3 py-1.5 text-[11px] font-medium text-ink-2">
                  <span aria-hidden>{levelGlyph}</span>
                  <span>L{level} · {levelLabel}</span>
                </span>
              )}
            </div>
            <span className="hidden items-center gap-2 text-xs text-mute sm:inline-flex">
              <span className="h-1.5 w-1.5 rounded-full bg-positive shadow-[0_0_8px_currentcolor]" />
              <T id="hub.rbacStatus" hideSecondary values={{ open: kpis.open, total: kpis.total }} />
            </span>
          </div>

          <div className="mt-7 grid gap-5 lg:grid-cols-12 lg:gap-6">
            <div className="flex min-w-0 flex-col lg:col-span-7">
              <div className="flex items-start gap-4">
                <span className="mt-1 inline-flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl border border-accent/30 bg-accent-soft/60 text-accent shadow-inner">
                  <GreetingIcon size={21} aria-hidden />
                </span>
                <div className="min-w-0">
                  <h1 className="page-title max-w-3xl text-ink">
                    <T
                      id={greetId}
                      values={{ name: fullname }}
                      variant="stacked"
                      primaryClassName="block"
                      secondaryClassName="mt-2 block text-sm font-medium tracking-normal text-ink-2"
                    />
                  </h1>
                  {dept && (
                    <p className="mt-2 text-sm leading-relaxed text-ink-2 sm:text-base">
                      <T id="hub.signedInTo" values={{ dept }} />
                      {actor?.employee_code && (
                        <>
                          <span className="mx-2 text-mute">·</span>
                          <span className="font-mono text-xs text-mute">{actor.employee_code}</span>
                        </>
                      )}
                    </p>
                  )}
                </div>
              </div>

              <div className="mt-7 grid grid-cols-3 divide-x divide-rule overflow-hidden rounded-2xl border border-rule bg-paper/35">
                <HeroMetric
                  label={<T id="hub.kpiAccessible" hideSecondary />}
                  value={kpis.open}
                  detail={<T id="hub.kpiOfTiles" hideSecondary values={{ total: kpis.total }} />}
                  tone="text-accent"
                />
                <HeroMetric
                  label={<T id="hub.kpiLocked" hideSecondary />}
                  value={kpis.locked}
                  detail={<T id="hub.kpiLockedCaption" hideSecondary />}
                  tone="text-critical"
                />
                <HeroMetric
                  label={<T id="hub.kpiPending" hideSecondary />}
                  value={actionQueue.state === 'error' ? '!' : actionQueue.total}
                  detail={actionQueue.state === 'error'
                    ? <T id="hub.actionErrorTitle" hideSecondary />
                    : actionQueue.total === 0
                    ? <T id="hub.kpiPendingClear" hideSecondary />
                    : <T id="hub.kpiPendingReview" hideSecondary />}
                  tone={actionQueue.state === 'error' ? 'text-critical' : actionQueue.total === 0 ? 'text-positive' : 'text-caution'}
                />
              </div>

              <div className="mt-4 grid gap-2 sm:grid-cols-3">
                {QUICK_ACTIONS.map((a) => {
                  const Icon = a.icon;
                  const tone = a.tone === 'accent' ? 'text-accent' : a.tone === 'info' ? 'text-info' : 'text-caution';
                  const handler =
                    a.key === 'search' ? onOpenCommand :
                    a.key === 'persona'
                      ? (onOpenPersona ?? (() => window.dispatchEvent(new Event('folio:open-persona'))))
                      : (onOpenNotifications ?? (() => window.dispatchEvent(new Event('folio:open-notifications'))));
                  return (
                    <button
                      key={a.key}
                      type="button"
                      onClick={handler}
                      className="panel-interactive group relative flex min-w-0 items-center gap-3 rounded-xl px-3 py-3 text-left"
                    >
                      <span className={`inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-rule bg-paper-2/55 ${tone}`}>
                        <Icon size={16} aria-hidden />
                      </span>
                      <span className="min-w-0">
                        <span className="block truncate text-xs font-semibold text-ink"><T id={a.id} hideSecondary /></span>
                        <span className="mt-0.5 block text-[10px] font-medium uppercase tracking-[0.12em] text-mute"><T id={a.hintId} hideSecondary /></span>
                      </span>
                      {a.key === 'notifications' && typeof unreadCount === 'number' && unreadCount > 0 && (
                        <span className="absolute right-2 top-2 inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-full bg-critical px-1 font-mono text-[10px] font-bold text-paper shadow-md">
                          {unreadCount > 99 ? '99+' : unreadCount}
                        </span>
                      )}
                    </button>
                  );
                })}
              </div>
            </div>

            <div className="min-w-0 lg:col-span-5">
              <PendingApprovals queue={actionQueue} fmtMoney={fmtMoney} />
            </div>
          </div>
        </div>
      </section>
    </TileTooltipProvider>
  );
};

function HeroMetric({
  label,
  value,
  detail,
  tone,
}: {
  label: React.ReactNode;
  value: React.ReactNode;
  detail: React.ReactNode;
  tone: string;
}) {
  return (
    <div className="min-w-0 px-3 py-4 sm:px-4">
      <span className="block truncate text-[10px] font-semibold uppercase tracking-[0.14em] text-mute">{label}</span>
      <span className={`mt-1.5 block font-mono text-2xl font-semibold leading-none tracking-[-0.04em] ${tone}`}>{value}</span>
      <span className="mt-1.5 block truncate text-[11px] text-mute">{detail}</span>
    </div>
  );
}

interface PendingApprovalsProps {
  queue: ActionQueueSummary;
  fmtMoney: (n: number | string | null | undefined, currency?: string) => string;
}

const PENDING_HREF = '/inbox?view=actions&read=all&domain=all';

function timeAgo(iso: string | null | undefined): { id: string; values?: Record<string, number | string> } | null {
  if (!iso) return null;
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return null;
  const diff = Date.now() - t;
  const m = Math.floor(diff / 60000);
  if (m < 1) return { id: 'hub.timeAgoJustNow' };
  if (m < 60) return { id: 'hub.timeAgoMinutes', values: { n: m } };
  const h = Math.floor(m / 60);
  if (h < 24) return { id: 'hub.timeAgoHours', values: { n: h } };
  const d = Math.floor(h / 24);
  if (d < 7) return { id: 'hub.timeAgoDays', values: { n: d } };
  return null;
}

const PendingApprovals: React.FC<PendingApprovalsProps> = ({ queue, fmtMoney }) => {
  const items = queue.items;
  const isError = queue.state === 'error';

  return (
    <section
      aria-label="Open action queue"
      className="panel-interactive group relative block h-full min-h-[292px] w-full overflow-hidden rounded-2xl bg-paper/30"
    >
      <div className="flex items-center justify-between gap-2 px-4 sm:px-5 pt-4 sm:pt-5">
        <div className="inline-flex items-center gap-1.5 text-xs font-mono font-bold uppercase tracking-widest text-ink">
          <ShoppingCart size={14} aria-hidden className="text-accent" />
          <T id="hub.actionTitle" hideSecondary />
        </div>
        <div className="inline-flex items-center gap-1.5 text-xs font-mono font-bold uppercase tracking-widest text-ink-2">
          <span className={`min-w-[18px] h-[18px] inline-flex items-center justify-center rounded-full px-1.5 text-xs font-black ${isError ? 'bg-critical text-paper border border-critical' : queue.total === 0 ? 'bg-positive text-paper border border-positive' : 'bg-accent text-paper border border-accent'}`}>
            {isError ? '!' : queue.total}
          </span>
        </div>
      </div>

      {isError ? (
        <div className="flex min-h-[210px] flex-col items-center justify-center px-5 py-8 text-center">
          <span className="mb-3 inline-flex h-12 w-12 items-center justify-center rounded-full border border-critical/30 bg-critical-soft/55 text-critical">
            <Bell size={23} aria-hidden />
          </span>
          <div className="text-sm font-semibold text-ink"><T id="hub.actionErrorTitle" hideSecondary /></div>
          <div className="mt-1 max-w-[30ch] text-sm text-mute"><T id="hub.actionErrorBody" hideSecondary /></div>
        </div>
      ) : items.length === 0 ? (
        <div className="flex min-h-[210px] flex-col items-center justify-center px-5 py-8 text-center">
          <span className="mb-3 inline-flex h-12 w-12 items-center justify-center rounded-full border border-positive/30 bg-positive-soft/55 text-positive">
            <CheckCircle2 size={23} aria-hidden />
          </span>
          <div className="text-sm font-semibold text-ink"><T id="hub.actionEmptyTitle" hideSecondary /></div>
          <div className="mt-1 max-w-[30ch] text-sm text-mute">
            <T id="hub.actionEmptyBody" hideSecondary />
          </div>
        </div>
      ) : (
        <ul className="divide-y divide-rule mt-2">
          {items.map((item) => {
            const ago = timeAgo(item.createdAt);
            return (
              <li key={`${item.waybillId}:${item.stageKey}`}>
                <Link href={item.href} className="flex items-center gap-3 px-4 sm:px-5 py-2.5 transition-colors hover:bg-paper-2/40">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="font-mono text-xs text-mute shrink-0">{item.waybillId}</span>
                      <span className="truncate text-[13px] font-bold text-ink">{item.counterparty ?? item.origin.toUpperCase()}</span>
                    </div>
                    <div className="mt-0.5 flex items-center gap-1.5 text-xs text-mute font-mono truncate">
                      <span className="truncate">{item.message}</span>
                      <span className="text-mute">·</span>
                      {ago ? <span className="shrink-0" suppressHydrationWarning><T id={ago.id} hideSecondary values={ago.values} /></span> : null}
                    </div>
                  </div>
                  <div className="shrink-0 flex flex-col items-end gap-1">
                    <span className="inline-flex items-center rounded-full border border-caution/40 bg-caution-soft/70 px-2 py-0.5 text-xs font-mono font-bold uppercase tracking-widest text-caution">
                      {item.stageKey.replace(/_/g, ' ')}
                    </span>
                    <span className="text-sm font-mono font-bold text-ink tabular-nums">{fmtMoney(item.totalAmount, item.currency)}</span>
                  </div>
                </Link>
              </li>
            );
          })}
        </ul>
      )}

      <div className="flex items-center justify-between gap-2 px-4 sm:px-5 py-3 border-t border-rule/70 mt-1">
        <span className="text-xs font-mono text-mute truncate">
          {isError ? <T id="hub.actionFooterError" hideSecondary /> : items.length === 0 ? <T id="hub.actionFooterIdle" hideSecondary /> : <T id="hub.actionFooter" hideSecondary values={{ shown: items.length, total: queue.total }} />}
        </span>
        <Link href={PENDING_HREF} className="text-xs font-mono font-bold uppercase tracking-widest text-accent transition-colors hover:text-accent-strong">
          <T id="hub.actionViewAll" hideSecondary />
        </Link>
      </div>
    </section>
  );
};

export default HubHero;
