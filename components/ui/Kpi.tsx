'use client';

import React from 'react';

export type KpiAccent =
  | 'emerald'
  | 'cyan'
  | 'indigo'
  | 'amber'
  | 'rose'
  | 'purple'
  | 'slate';

const ACCENT: Record<KpiAccent, { text: string; border: string }> = {
  emerald: { text: 'text-emerald-400', border: 'border-emerald-500/30' },
  cyan: { text: 'text-cyan-400', border: 'border-cyan-500/30' },
  indigo: { text: 'text-indigo-400', border: 'border-indigo-500/30' },
  amber: { text: 'text-amber-400', border: 'border-amber-500/30' },
  rose: { text: 'text-rose-400', border: 'border-rose-500/30' },
  purple: { text: 'text-purple-400', border: 'border-purple-500/30' },
  slate: { text: 'text-slate-400', border: 'border-slate-500/30' },
};

export interface KpiProps {
  label: React.ReactNode;
  value: React.ReactNode;
  accent?: KpiAccent;
  size?: 'sm' | 'md';
  caption?: React.ReactNode;
  valueClassName?: string;
}

export const Kpi: React.FC<KpiProps> = ({ label, value, accent = 'slate', size = 'md', caption, valueClassName }) => {
  const a = ACCENT[accent];
  const padding = size === 'sm' ? 'p-3' : 'p-4';
  const textSize = size === 'sm' ? 'text-xl' : 'text-2xl';
  return (
    <div className={`bg-slate-900/60 ${padding} rounded-2xl border ${a.border} relative`}>
      <span className="text-xs text-slate-400 uppercase tracking-widest font-mono font-black block">
        {label}
      </span>
      <span className={`${textSize} font-black font-mono mt-2 block ${valueClassName ?? a.text}`}>
        {value}
      </span>
      {caption && (
        <span className="text-xs text-slate-500 font-mono mt-1 block">{caption}</span>
      )}
    </div>
  );
};

export default Kpi;