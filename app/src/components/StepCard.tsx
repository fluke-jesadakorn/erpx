import React from 'react';

export function StepBadge({
  n,
  done,
  active,
}: {
  n: number;
  done: boolean;
  active: boolean;
}) {
  return (
    <span
      className={[
        'shrink-0 inline-flex items-center justify-center w-7 h-7 rounded-full text-xs font-mono font-bold border transition-colors',
        done
          ? 'bg-emerald-500 text-slate-950 border-emerald-400 shadow-[0_0_0_3px_rgba(16,185,129,0.18)]'
          : active
          ? 'bg-indigo-500 text-white border-indigo-300 shadow-[0_0_0_3px_rgba(99,102,241,0.25)]'
          : 'bg-slate-900 text-slate-500 border-slate-700',
      ].join(' ')}
      aria-current={active ? 'step' : undefined}
    >
      {done ? '✓' : n}
    </span>
  );
}

export function StepCard({
  n,
  icon,
  title,
  titleTh,
  hint,
  done,
  active,
  tone = 'indigo',
  accent,
  children,
  badge,
  dimmed = false,
}: {
  n: number;
  icon: string;
  title: React.ReactNode;
  titleTh: React.ReactNode;
  hint: React.ReactNode;
  done: boolean;
  active: boolean;
  tone?: 'indigo' | 'cyan' | 'emerald' | 'amber' | 'slate';
  accent?: 'your-turn' | null;
  children: React.ReactNode;
  badge?: React.ReactNode;
  dimmed?: boolean;
}) {
  const ring = {
    indigo: 'border-indigo-500/30 shadow-[0_0_0_1px_rgba(99,102,241,0.15)]',
    cyan: 'border-cyan-500/30 shadow-[0_0_0_1px_rgba(6,182,212,0.15)]',
    emerald: 'border-emerald-500/30 shadow-[0_0_0_1px_rgba(16,185,129,0.18)]',
    amber: 'border-amber-500/30 shadow-[0_0_0_1px_rgba(245,158,11,0.15)]',
    slate: 'border-slate-700/60 shadow-[0_0_0_1px_rgba(71,85,105,0.25)]',
  }[tone];

  const head = {
    indigo: 'from-indigo-500/15 to-indigo-500/0 text-indigo-200',
    cyan: 'from-cyan-500/15 to-cyan-500/0 text-cyan-200',
    emerald: 'from-emerald-500/20 to-emerald-500/0 text-emerald-200',
    amber: 'from-amber-500/20 to-amber-500/0 text-amber-200',
    slate: 'from-slate-800/40 to-slate-800/0 text-slate-300',
  }[tone];

  const accentRing =
    accent === 'your-turn'
      ? 'border-cyan-400/70 shadow-[0_0_0_1px_rgba(6,182,212,0.25),0_16px_44px_-14px_rgba(6,182,212,0.4)]'
      : '';

  const accentOverlay =
    accent === 'your-turn'
      ? 'before:absolute before:inset-0 before:rounded-2xl before:bg-[radial-gradient(ellipse_at_top_left,rgba(6,182,212,0.12),transparent_55%)] before:pointer-events-none'
      : '';

  return (
    <article
      className={[
        'relative overflow-hidden rounded-2xl border bg-slate-950/55 backdrop-blur-sm transition-opacity',
        ring,
        accentRing,
        accentOverlay,
        dimmed
          ? 'opacity-50 grayscale transition duration-200 hover:opacity-100 hover:grayscale-0'
          : '',
      ].join(' ')}
      data-step={n}
      data-dimmed={dimmed ? 'true' : undefined}
      data-accent={accent ?? undefined}
    >
      <header
        className={[
          'relative flex items-center gap-3 px-4 py-3 border-b border-slate-800/80 bg-gradient-to-r',
          head,
          accent === 'your-turn' ? 'border-cyan-500/25' : '',
        ].join(' ')}
      >
        <StepBadge key={`badge-${n}`} n={n} done={done} active={active} />
        <div key={`title-${n}`} className="min-w-0 flex-1">
          <div className="flex items-center gap-2 flex-wrap">
            <span aria-hidden className="text-base leading-none">
              {icon}
            </span>
            <h3 className="text-sm font-bold text-white leading-tight">
              {title}
            </h3>
            {titleTh ? (
              <span className="text-sm text-slate-500 font-mono">
                {titleTh}
              </span>
            ) : null}
            {accent === 'your-turn' && (
              <span className="relative inline-flex items-center gap-1.5 overflow-hidden rounded-full border border-cyan-400/60 bg-cyan-500/15 px-2.5 py-0.5 text-xs font-mono font-bold uppercase tracking-widest text-cyan-100 shadow-[0_0_10px_-2px_rgba(6,182,212,0.5)]">
                <span aria-hidden className="relative inline-flex h-1.5 w-1.5">
                  <span className="absolute inset-0 animate-ping rounded-full bg-cyan-300 opacity-75" />
                  <span className="relative inline-block h-1.5 w-1.5 rounded-full bg-cyan-200" />
                </span>
                Your turn
              </span>
            )}
          </div>
          <p className="text-sm text-slate-400 mt-0.5 leading-snug">{hint}</p>
        </div>
        {badge}
      </header>
      <div className="relative p-6">{children}</div>
    </article>
  );
}

export function fmtTime(d: Date | string | null | undefined): string {
  if (!d) return '—';
  const date = typeof d === 'string' ? new Date(d) : d;
  if (Number.isNaN(date.getTime())) return '—';
  return date.toLocaleTimeString('en-GB', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
}
