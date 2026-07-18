'use client';

import React from 'react';
import Link from 'next/link';
import { Kpi } from './ui/Kpi';
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
  LayoutDashboard,
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
  key: 'search' | 'dashboard' | 'persona' | 'notifications';
  id: string;
  hintId: string;
  icon: LucideIcon;
  tone: 'accent' | 'info' | 'caution';
  href?: string;
  isButton: boolean;
}> = [
  { key: 'search',       id: 'hub.quickSearchLabel',        hintId: 'hub.quickSearchHint',        icon: Search,          tone: 'accent',  isButton: true },
  { key: 'dashboard',    id: 'hub.quickDashboardLabel',     hintId: 'hub.quickDashboardHint',     icon: LayoutDashboard, tone: 'info',    isButton: false, href: '/' },
  { key: 'persona',      id: 'hub.quickPersonaLabel',       hintId: 'hub.quickPersonaHint',       icon: UserRound,       tone: 'accent',  isButton: true },
  { key: 'notifications',id: 'hub.quickNotificationsLabel', hintId: 'hub.quickNotificationsHint', icon: Bell,            tone: 'caution', isButton: true },
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

        <div className="relative z-10 space-y-6 p-5 sm:p-7 lg:p-8">
        <div className="flex items-center justify-between gap-3 flex-wrap">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="inline-flex items-center gap-1.5 rounded-full border border-accent bg-accent px-2.5 py-1 text-xs font-mono font-bold uppercase tracking-widest text-paper">
              <span className="w-1.5 h-1.5 rounded-full bg-accent shadow-[0_0_8px_currentcolor] animate-pulse" />
              <T id="hub.live" />
            </span>
            {role && (
              <span className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-mono font-bold uppercase tracking-widest ${roleBadge(role)}`}>
                <span aria-hidden>{roleGlyph(role)}</span>
                <span>{roleLabel(role)}</span>
              </span>
            )}
            {level != null && levelLabel && (
              <span className="inline-flex items-center gap-1 rounded-full border border-rule bg-paper-2/60 px-2 py-0.5 text-xs font-mono font-bold uppercase tracking-widest text-ink-2">
                <span aria-hidden>{levelGlyph}</span>
                <span>L{level} · {levelLabel}</span>
              </span>
            )}
          </div>
          <div className="text-xs font-mono text-mute hidden sm:flex items-center gap-1.5">
            <span className="w-1.5 h-1.5 rounded-full bg-positive shadow-[0_0_8px_currentcolor]" />
            <T id="hub.rbacStatus" values={{ open: kpis.open, total: kpis.total }} />
          </div>
        </div>

        <div className="space-y-1.5">
          <h1 className="page-title flex items-center gap-3 text-ink">
            <span className="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-2xl border border-accent/30 bg-accent-soft/60 text-accent"><GreetingIcon size={20} aria-hidden /></span>
             <T id={greetId} values={{ name: fullname }} />
            <span className="text-mute">.</span>
          </h1>
          {dept && (
            <p className="text-sm sm:text-base text-ink-2 leading-relaxed">
               <T id="hub.signedInTo" values={{ dept }} />
              {actor?.employee_code && (
                <>
                  <span className="text-mute mx-1.5">·</span>
                  <span className="font-mono text-xs text-mute">{actor.employee_code}</span>
                </>
              )}
            </p>
          )}
        </div>

        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5 sm:gap-3">
          <Kpi
            label={<T id="hub.kpiAccessible" hideSecondary />}
            value={kpis.open}
            tone="accent"
            caption={<T id="hub.kpiOfTiles" hideSecondary values={{ total: kpis.total }} />}
          />
          <Kpi
            label={<T id="hub.kpiLocked" hideSecondary />}
            value={kpis.locked}
            tone="critical"
            caption={<T id="hub.kpiLockedCaption" hideSecondary />}
          />
          <Kpi
            label={<T id="hub.kpiGroups" hideSecondary />}
            value={kpis.groups}
            tone="info"
            caption={<T id="hub.kpiGroupsCaption" hideSecondary />}
          />
          <Kpi
            label={<T id="hub.kpiPending" hideSecondary />}
            value={pending.length}
            tone="positive"
            caption={pending.length === 0
              ? <T id="hub.kpiPendingClear" hideSecondary />
              : <T id="hub.kpiPendingReview" hideSecondary />
            }
            valueClassName="text-base"
          />
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-5 gap-3 sm:gap-4">
          <div className="lg:col-span-3">
            <PendingApprovals prs={pending} fmtMoney={fmtMoney} />
          </div>
          <div className="lg:col-span-2 grid grid-cols-2 lg:grid-cols-1 gap-2.5 sm:gap-3">
            {QUICK_ACTIONS.map((a) => {
              const Icon = a.icon;
              const accentText =
                a.key === 'search' ? 'text-accent' :
                a.key === 'dashboard' ? 'text-info' :
                a.key === 'persona' ? 'text-accent' :
                'text-caution';
              const inner = (
                <div className={`panel-interactive relative h-full rounded-2xl p-3.5 sm:p-4 ${a.tone === 'accent' ? 'border-accent/30' : a.tone === 'info' ? 'border-info/30' : 'border-caution/30'}`}>
                  <div className="flex items-start justify-between gap-2">
                    <span className={`inline-flex h-9 w-9 items-center justify-center rounded-xl border border-rule bg-paper-2/60 ${accentText}`}><Icon size={17} aria-hidden /></span>
                    <span className={`text-xs font-mono font-bold uppercase tracking-widest ${accentText}`}>
                      <T id={a.hintId} hideSecondary />
                    </span>
                  </div>
                  <div className="mt-3 sm:mt-4">
                    <div className="text-[13px] font-black text-ink leading-tight"><T id={a.id} hideSecondary /></div>
                  </div>
                  {a.key === 'notifications' && typeof unreadCount === 'number' && unreadCount > 0 && (
                    <span className="absolute top-2 right-2 min-w-[18px] h-[18px] inline-flex items-center justify-center rounded-full bg-critical text-paper text-xs font-black font-mono px-1 shadow-md">
                      {unreadCount > 99 ? '99+' : unreadCount}
                    </span>
                  )}
                </div>
              );
              if (a.isButton) {
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
                    aria-label={a.id}
                    className="text-left h-full"
                  >
                    {inner}
                  </button>
                );
              }
              return (
                <Link key={a.key} href={a.href!} aria-label={a.id} className="h-full">
                  {inner}
                </Link>
              );
            })}
          </div>
        </div>
      </div>
    </section>
    </TileTooltipProvider>
  );
};

interface PendingApprovalsProps {
  prs: any[];
  fmtMoney: (n: number | string | null | undefined, currency?: string) => string;
}

const PENDING_HREF = '/subordinate-prs';

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
  const items = (prs ?? []).slice(0, 4);

  return (
    <Link
      href={PENDING_HREF}
      aria-label="Open pending approvals"
      className="panel-interactive group relative block h-full w-full overflow-hidden rounded-2xl"
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
        <div className="flex flex-col items-center justify-center text-center px-5 py-8 min-h-[160px]">
          <CheckCircle2 size={28} className="mb-2 text-positive" aria-hidden />
          <div className="text-sm font-bold text-ink"><T id="hub.pendingEmptyTitle" hideSecondary /></div>
          <div className="text-sm text-mute font-mono mt-1 max-w-[28ch]">
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
        <span className="text-xs font-mono font-bold uppercase tracking-widest text-accent group-hover:text-accent-soft transition-colors">
          <T id="hub.pendingViewAll" hideSecondary />
        </span>
      </div>
    </Link>
  );
};

export default HubHero;
