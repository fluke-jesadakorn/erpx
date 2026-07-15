import React from 'react';
import { fmtSize } from './ui';

export type ZoneTone = 'cyan' | 'emerald' | 'indigo' | 'amber' | 'rose' | 'slate' | 'fuchsia';

interface ToneClasses {
  border: string;
  bg: string;
  icon: string;
  label: string;
  count: string;
}

const TONES: Record<ZoneTone, ToneClasses> = {
  cyan: {
    border: 'border-l-cyan-400/60',
    bg: 'bg-cyan-500/5',
    icon: 'text-cyan-300',
    label: 'text-cyan-300/90',
    count: 'bg-cyan-500/20 text-cyan-200 border-cyan-400/40',
  },
  emerald: {
    border: 'border-l-emerald-400/60',
    bg: 'bg-emerald-500/5',
    icon: 'text-emerald-300',
    label: 'text-emerald-300/90',
    count: 'bg-emerald-500/20 text-emerald-200 border-emerald-400/40',
  },
  indigo: {
    border: 'border-l-indigo-400/60',
    bg: 'bg-indigo-500/5',
    icon: 'text-indigo-300',
    label: 'text-indigo-300/90',
    count: 'bg-indigo-500/20 text-indigo-200 border-indigo-400/40',
  },
  amber: {
    border: 'border-l-amber-400/60',
    bg: 'bg-amber-500/5',
    icon: 'text-amber-300',
    label: 'text-amber-300/90',
    count: 'bg-amber-500/20 text-amber-200 border-amber-400/40',
  },
  rose: {
    border: 'border-l-rose-400/60',
    bg: 'bg-rose-500/5',
    icon: 'text-rose-300',
    label: 'text-rose-300/90',
    count: 'bg-rose-500/20 text-rose-200 border-rose-400/40',
  },
  fuchsia: {
    border: 'border-l-fuchsia-400/60',
    bg: 'bg-fuchsia-500/5',
    icon: 'text-fuchsia-300',
    label: 'text-fuchsia-300/90',
    count: 'bg-fuchsia-500/20 text-fuchsia-200 border-fuchsia-400/40',
  },
  slate: {
    border: 'border-l-slate-500/60',
    bg: 'bg-slate-500/5',
    icon: 'text-slate-300',
    label: 'text-slate-300/90',
    count: 'bg-slate-700/40 text-slate-200 border-slate-600/60',
  },
};

interface Props {
  icon: React.ReactNode;
  label: string;
  count?: number | string;
  meta?: string;
  tone?: ZoneTone;
  dense?: boolean;
  children: React.ReactNode;
}

export function ZoneSection({
  icon,
  label,
  count,
  meta,
  tone = 'slate',
  dense = false,
  children,
}: Props) {
  const t = TONES[tone];
  const sizeBytes = typeof count === 'number' && label.toLowerCase().includes('document') ? count : null;
  const displayCount =
    typeof count === 'number' && sizeBytes !== null ? fmtSize(count) : count;
  return (
    <section
      className={[
        'relative rounded-xl border border-slate-800/70 border-l-4',
        t.border,
        t.bg,
        dense ? 'p-3' : 'p-4',
      ].join(' ')}
    >
      <header className="mb-2.5 flex items-center gap-2">
        <span aria-hidden className={`text-sm leading-none ${t.icon}`}>
          {icon}
        </span>
        <span
          className={`text-xs font-mono uppercase tracking-widest ${t.label}`}
        >
          {label}
        </span>
        {displayCount !== undefined && (
          <span
            className={`ml-1 inline-flex items-center rounded-full border px-1.5 py-0.5 text-xs font-mono font-bold ${t.count}`}
          >
            {displayCount}
          </span>
        )}
        {meta && (
          <span className="ml-auto truncate text-xs font-mono text-slate-500">
            {meta}
          </span>
        )}
      </header>
      <div className={dense ? '' : ''}>{children}</div>
    </section>
  );
}