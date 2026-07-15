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
} from '@/lib/hero';

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
  label: string;
  hint: string;
  icon: string;
  tone: string;
  href?: string;
  isButton: boolean;
}> = [
  { key: 'search',       label: 'Quick search',    hint: '⌘K',          icon: '🔍', tone: 'from-indigo-500/20 to-indigo-900/20 border-indigo-500/30', isButton: true },
  { key: 'dashboard',    label: 'Dashboard',       hint: 'overview',    icon: '📊', tone: 'from-cyan-500/20 to-cyan-900/20 border-cyan-500/30',     isButton: false, href: '/' },
  { key: 'persona',      label: 'Switch persona',  hint: 'dev tool',    icon: '👤', tone: 'from-purple-500/20 to-purple-900/20 border-purple-500/30', isButton: true },
  { key: 'notifications',label: 'Notifications',   hint: 'inbox',       icon: '🔔', tone: 'from-amber-500/20 to-amber-900/20 border-amber-500/30',   isButton: true },
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

  const fullname = (actor?.fullname || '').trim() || 'there';
  const greetingKey: GreetingKey = initialGreetingKey ?? timeGreeting();
  const greet = greetingLine(fullname, greetingKey);
  const role = actor?.role_name ?? undefined;
  const dept = actor?.dept_group_name || actor?.department || null;
  const level = actor?.level ?? null;
  const levelLabel = level != null ? staffLevelLabel(level) : null;
  const levelGlyph = level != null ? staffLevelGlyph(level) : '·';

  const greetingEmoji =
    greetingKey === 'morning' ? '☀️'
    : greetingKey === 'afternoon' ? '🌤️'
    : '🌙';

  return (
    <TileTooltipProvider>
      <section
        aria-label="Hub hero"
        className="relative overflow-hidden rounded-3xl border border-slate-800/80 glass-panel-heavy shadow-2xl shadow-black/50 animate-fade-in"
      >
        <HubAmbient variant="hero" />

        <div className="relative z-10 p-5 sm:p-7 lg:p-9 space-y-6">
        <div className="flex items-center justify-between gap-3 flex-wrap">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="inline-flex items-center gap-1.5 rounded-full border border-indigo-500/30 bg-indigo-500/10 px-2.5 py-1 text-xs font-mono font-bold uppercase tracking-widest text-indigo-200">
              <span className="w-1.5 h-1.5 rounded-full bg-indigo-400 shadow-[0_0_8px_currentcolor] animate-pulse" />
              Live · Folio
            </span>
            {role && (
              <span className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-mono font-bold uppercase tracking-widest ${roleBadge(role)}`}>
                <span aria-hidden>{roleGlyph(role)}</span>
                <span>{roleLabel(role)}</span>
              </span>
            )}
            {level != null && levelLabel && (
              <span className="inline-flex items-center gap-1 rounded-full border border-slate-700 bg-slate-900/60 px-2 py-0.5 text-xs font-mono font-bold uppercase tracking-widest text-slate-300">
                <span aria-hidden>{levelGlyph}</span>
                <span>L{level} · {levelLabel}</span>
              </span>
            )}
          </div>
          <div className="text-xs font-mono text-slate-500 hidden sm:flex items-center gap-1.5">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 shadow-[0_0_8px_currentcolor]" />
            <span>RBAC-aware · {kpis.open} of {kpis.total} accessible</span>
          </div>
        </div>

        <div className="space-y-1.5">
          <h1 className="text-2xl sm:text-3xl lg:text-4xl font-black tracking-tight text-white leading-tight">
            <span aria-hidden className="mr-1.5">{greetingEmoji}</span>
            {greet}
            <span className="text-slate-500">.</span>
          </h1>
          {dept && (
            <p className="text-sm sm:text-base text-slate-400 leading-relaxed">
              Signed in to <span className="text-slate-200 font-bold">{dept}</span>
              {actor?.employee_code && (
                <>
                  <span className="text-slate-600 mx-1.5">·</span>
                  <span className="font-mono text-xs text-slate-500">{actor.employee_code}</span>
                </>
              )}
            </p>
          )}
        </div>

        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5 sm:gap-3">
          <Kpi
            label="Accessible"
            value={kpis.open}
            accent="indigo"
            caption={`of ${kpis.total} tiles`}
          />
          <Kpi
            label="Locked"
            value={kpis.locked}
            accent="rose"
            caption="request access"
          />
          <Kpi
            label="Groups"
            value={kpis.groups}
            accent="cyan"
            caption="active in catalog"
          />
          <Kpi
            label="Pending"
            value={pending.length}
            accent="emerald"
            caption={pending.length === 0 ? 'all clear' : 'awaiting review'}
            valueClassName="text-base"
          />
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-5 gap-3 sm:gap-4">
          <div className="lg:col-span-3">
            <PendingApprovals prs={pending} />
          </div>
          <div className="lg:col-span-2 grid grid-cols-2 lg:grid-cols-1 gap-2.5 sm:gap-3">
            {QUICK_ACTIONS.map((a) => {
              const accentText =
                a.key === 'search' ? 'text-indigo-300' :
                a.key === 'dashboard' ? 'text-cyan-300' :
                a.key === 'persona' ? 'text-purple-300' :
                'text-amber-300';
              const inner = (
                <div className={`relative h-full rounded-2xl border bg-gradient-to-br p-3.5 sm:p-4 transition-all hover:scale-[1.02] hover:-translate-y-0.5 hover:shadow-lg hover:shadow-black/40 ${a.tone}`}>
                  <div className="flex items-start justify-between gap-2">
                    <span className="text-2xl drop-shadow" aria-hidden>{a.icon}</span>
                    <span className={`text-xs font-mono font-bold uppercase tracking-widest ${accentText}`}>
                      {a.hint}
                    </span>
                  </div>
                  <div className="mt-3 sm:mt-4">
                    <div className="text-[13px] font-black text-white leading-tight">{a.label}</div>
                  </div>
                  {a.key === 'notifications' && typeof unreadCount === 'number' && unreadCount > 0 && (
                    <span className="absolute top-2 right-2 min-w-[18px] h-[18px] inline-flex items-center justify-center rounded-full bg-rose-500 text-white text-xs font-black font-mono px-1 shadow-md">
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
                    aria-label={a.label}
                    className="text-left h-full"
                  >
                    {inner}
                  </button>
                );
              }
              return (
                <Link key={a.key} href={a.href!} aria-label={a.label} className="h-full">
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
}

const PENDING_HREF = '/subordinate-prs';

const STAGE_PILL: Record<string, { label: string; tone: string }> = {
  supervisor_review:         { label: 'Supervisor',  tone: 'border-amber-500/40   text-amber-200   bg-amber-500/10'   },
  head_review:               { label: 'Head',        tone: 'border-amber-500/40   text-amber-200   bg-amber-500/10'   },
  account_officer_review:    { label: 'Accountant',  tone: 'border-cyan-500/40    text-cyan-200    bg-cyan-500/10'    },
  account_supervisor_review: { label: 'Acct. Sup.',  tone: 'border-cyan-500/40    text-cyan-200    bg-cyan-500/10'    },
  accounting_review:         { label: 'Accounting',  tone: 'border-cyan-500/40    text-cyan-200    bg-cyan-500/10'    },
  cfo_review:                { label: 'CFO',         tone: 'border-purple-500/40  text-purple-200  bg-purple-500/10'  },
  ceo_review:                { label: 'CEO',         tone: 'border-purple-500/40  text-purple-200  bg-purple-500/10'  },
  finance_review:            { label: 'Finance',     tone: 'border-purple-500/40  text-purple-200  bg-purple-500/10'  },
  po_pending:                { label: 'PO Pending',  tone: 'border-indigo-500/40  text-indigo-200  bg-indigo-500/10'  },
};

function timeAgo(iso: string | null | undefined): string {
  if (!iso) return '';
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return '';
  const diff = Date.now() - t;
  const m = Math.floor(diff / 60000);
  if (m < 1) return 'just now';
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d}d ago`;
  return new Date(iso).toLocaleDateString();
}

function fmtMoney(n: number | string | null | undefined, currency: string | null | undefined): string {
  const num = Number(n ?? 0);
  if (!Number.isFinite(num)) return '—';
  const cur = currency || 'THB';
  try {
    return new Intl.NumberFormat('en-US', { maximumFractionDigits: 0 }).format(num) + ' ' + cur;
  } catch {
    return `${num} ${cur}`;
  }
}

const PendingApprovals: React.FC<PendingApprovalsProps> = ({ prs }) => {
  const items = (prs ?? []).slice(0, 4);

  return (
    <Link
      href={PENDING_HREF}
      aria-label="Open pending approvals"
      className="group relative block w-full h-full rounded-2xl overflow-hidden border border-slate-800/80 bg-slate-950/50 transition-all hover:border-slate-700 hover:shadow-2xl hover:shadow-black/60"
    >
      <div className="flex items-center justify-between gap-2 px-4 sm:px-5 pt-4 sm:pt-5">
        <div className="inline-flex items-center gap-1.5 text-xs font-mono font-bold uppercase tracking-widest text-indigo-200">
          <span aria-hidden>🛒</span>
          Pending approvals
        </div>
        <div className="inline-flex items-center gap-1.5 text-xs font-mono font-bold uppercase tracking-widest text-slate-400">
          <span className={`min-w-[18px] h-[18px] inline-flex items-center justify-center rounded-full px-1.5 text-xs font-black ${items.length === 0 ? 'bg-emerald-500/20 text-emerald-200 border border-emerald-500/40' : 'bg-indigo-500/20 text-indigo-200 border border-indigo-500/40'}`}>
            {items.length}
          </span>
        </div>
      </div>

      {items.length === 0 ? (
        <div className="flex flex-col items-center justify-center text-center px-5 py-8 min-h-[160px]">
          <div className="text-3xl mb-2 opacity-70" aria-hidden>✅</div>
          <div className="text-sm font-bold text-slate-200">All clear</div>
          <div className="text-sm text-slate-500 font-mono mt-1 max-w-[28ch]">
            No purchase requests waiting on your approval.
          </div>
        </div>
      ) : (
        <ul className="divide-y divide-slate-800/70 mt-2">
          {items.map((pr) => {
            const pill = STAGE_PILL[pr.status] ?? { label: pr.status ?? 'pending', tone: 'border-slate-600 text-slate-300 bg-slate-700/30' };
            const vendor = pr.vendor_name || `PR-${pr.id}`;
            return (
              <li key={pr.id} className="flex items-center gap-3 px-4 sm:px-5 py-2.5 transition-colors hover:bg-slate-900/40">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="font-mono text-xs text-slate-500 shrink-0">PR-{pr.id}</span>
                    <span className="truncate text-[13px] font-bold text-white">{vendor}</span>
                  </div>
                  <div className="mt-0.5 flex items-center gap-1.5 text-xs text-slate-500 font-mono truncate">
                    <span className="truncate">{pr.requester_name ?? '—'}</span>
                    {pr.requester_dept_group_name && (
                      <>
                        <span className="text-slate-700">·</span>
                        <span className="truncate">{pr.requester_dept_group_name}</span>
                      </>
                    )}
                    <span className="text-slate-700">·</span>
                    <span className="shrink-0">{timeAgo(pr.created_at)}</span>
                  </div>
                </div>
                <div className="shrink-0 flex flex-col items-end gap-1">
                  <span className={`inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-mono font-bold uppercase tracking-widest ${pill.tone}`}>
                    {pill.label}
                  </span>
                  <span className="text-sm font-mono font-bold text-slate-200 tabular-nums">
                    {fmtMoney(pr.total_estimate, pr.currency)}
                  </span>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      <div className="flex items-center justify-between gap-2 px-4 sm:px-5 py-3 border-t border-slate-800/70 mt-1">
        <span className="text-xs font-mono text-slate-500 truncate">
          {items.length === 0
            ? 'Awaiting new submissions'
            : `Top ${items.length} of ${prs.length} waiting`}
        </span>
        <span className="text-xs font-mono font-bold uppercase tracking-widest text-indigo-300 group-hover:text-indigo-200 transition-colors">
          View all →
        </span>
      </div>
    </Link>
  );
};

export default HubHero;
