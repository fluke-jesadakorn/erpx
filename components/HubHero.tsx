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
  pickPendingApprovals,
  timeGreeting,
  type GreetingKey,
} from '@/hero';
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
  pendingPrs: any[];
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
  pendingPrs,
  isLocked,
  onOpenCommand,
  onOpenPersona,
  onOpenNotifications,
  unreadCount,
  initialGreetingKey,
}) => {
  const kpis = kpiSummary(tiles, isLocked);
  const pending = pickPendingApprovals(pendingPrs ?? []);
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
                  value={pending.length}
                  detail={pending.length === 0
                    ? <T id="hub.kpiPendingClear" hideSecondary />
                    : <T id="hub.kpiPendingReview" hideSecondary />}
                  tone={pending.length === 0 ? 'text-positive' : 'text-caution'}
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
              <PendingApprovals prs={pending} fmtMoney={fmtMoney} />
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
  value: number;
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
  prs: any[];
  fmtMoney: (n: number | string | null | undefined, currency?: string) => string;
}

const PENDING_HREF = '/inbox?scope=waiting';

const STAGE_PILL: Record<string, { id: string; tone: string }> = {
  supervisor_review:         { id: 'hub.stage.supervisorReview',          tone: 'border-caution   text-paper   bg-caution'   },
  head_review:               { id: 'hub.stage.headReview',                tone: 'border-caution   text-paper   bg-caution'   },
  account_officer_review:    { id: 'hub.stage.accountOfficerReview',      tone: 'border-info    text-paper    bg-info'    },
  account_supervisor_review: { id: 'hub.stage.accountSupervisorReview',   tone: 'border-info    text-paper    bg-info'    },
  accounting_review:         { id: 'hub.stage.accountingReview',          tone: 'border-info    text-paper    bg-info'    },
  cfo_review:                { id: 'hub.stage.cfoReview',                 tone: 'border-accent  text-paper  bg-accent'  },
  ceo_review:                { id: 'hub.stage.ceoReview',                 tone: 'border-accent  text-paper  bg-accent'  },
  finance_review:            { id: 'hub.stage.financeReview',             tone: 'border-accent  text-paper  bg-accent'  },
  po_pending:                { id: 'hub.stage.poPending',                 tone: 'border-accent  text-paper  bg-accent'  },
};

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

const PendingApprovals: React.FC<PendingApprovalsProps> = ({ prs, fmtMoney }) => {
  const items = (prs ?? []).slice(0, 3);

  return (
    <Link
      href={PENDING_HREF}
      aria-label="Open pending approvals"
      className="panel-interactive group relative block h-full min-h-[292px] w-full overflow-hidden rounded-2xl bg-paper/30"
    >
      <div className="flex items-center justify-between gap-2 px-4 sm:px-5 pt-4 sm:pt-5">
        <div className="inline-flex items-center gap-1.5 text-xs font-mono font-bold uppercase tracking-widest text-ink">
          <ShoppingCart size={14} aria-hidden className="text-accent" />
          <T id="hub.pendingTitle" hideSecondary />
        </div>
        <div className="inline-flex items-center gap-1.5 text-xs font-mono font-bold uppercase tracking-widest text-ink-2">
          <span className={`min-w-[18px] h-[18px] inline-flex items-center justify-center rounded-full px-1.5 text-xs font-black ${items.length === 0 ? 'bg-positive text-paper border border-positive' : 'bg-accent text-paper border border-accent'}`}>
            {items.length}
          </span>
        </div>
      </div>

      {items.length === 0 ? (
        <div className="flex min-h-[210px] flex-col items-center justify-center px-5 py-8 text-center">
          <span className="mb-3 inline-flex h-12 w-12 items-center justify-center rounded-full border border-positive/30 bg-positive-soft/55 text-positive">
            <CheckCircle2 size={23} aria-hidden />
          </span>
          <div className="text-sm font-semibold text-ink"><T id="hub.pendingEmptyTitle" hideSecondary /></div>
          <div className="mt-1 max-w-[30ch] text-sm text-mute">
            <T id="hub.pendingEmptyBody" hideSecondary />
          </div>
        </div>
      ) : (
        <ul className="divide-y divide-rule mt-2">
          {items.map((pr) => {
            const pill = STAGE_PILL[pr.status] ?? { id: '', tone: 'border-rule text-ink-2 bg-paper-2/30' };
            const vendor = pr.vendor_name || `PR-${pr.id}`;
            const ago = timeAgo(pr.created_at);
            return (
              <li key={pr.id} className="flex items-center gap-3 px-4 sm:px-5 py-2.5 transition-colors hover:bg-paper-2/40">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="font-mono text-xs text-mute shrink-0">PR-{pr.id}</span>
                    <span className="truncate text-[13px] font-bold text-ink">{vendor}</span>
                  </div>
                  <div className="mt-0.5 flex items-center gap-1.5 text-xs text-mute font-mono truncate">
                    <span className="truncate">{pr.requester_name ?? '—'}</span>
                    {pr.requester_dept_group_name && (
                      <>
                        <span className="text-mute">·</span>
                        <span className="truncate">{pr.requester_dept_group_name}</span>
                      </>
                    )}
                    <span className="text-mute">·</span>
                    {ago ? (
                      <span className="shrink-0"><T id={ago.id} hideSecondary values={ago.values} /></span>
                    ) : (
                      <span className="shrink-0">{new Date(pr.created_at).toLocaleDateString()}</span>
                    )}
                  </div>
                </div>
                <div className="shrink-0 flex flex-col items-end gap-1">
                  <span className={`inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-mono font-bold uppercase tracking-widest ${pill.tone}`}>
                    <T id={pill.id} hideSecondary />
                  </span>
                  <span className="text-sm font-mono font-bold text-ink tabular-nums">
                    {fmtMoney(pr.total_estimate, pr.currency ?? undefined)}
                  </span>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      <div className="flex items-center justify-between gap-2 px-4 sm:px-5 py-3 border-t border-rule/70 mt-1">
        <span className="text-xs font-mono text-mute truncate">
          {items.length === 0 ? <T id="hub.pendingFooterIdle" hideSecondary /> : <T id="hub.pendingFooter" hideSecondary values={{ shown: items.length, total: prs.length }} />}
        </span>
        <span className="text-xs font-mono font-bold uppercase tracking-widest text-accent group-hover:text-accent transition-colors">
          <T id="hub.pendingViewAll" hideSecondary />
        </span>
      </div>
    </Link>
  );
};

export default HubHero;
