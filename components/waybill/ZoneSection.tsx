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
    border: 'border-l-info',
    bg: 'bg-info-soft',
    icon: 'text-info-strong',
    label: 'text-info-strong',
    count: 'bg-info text-paper border-info',
  },
  emerald: {
    border: 'border-l-positive',
    bg: 'bg-positive-soft',
    icon: 'text-positive-strong',
    label: 'text-positive-strong',
    count: 'bg-positive text-paper border-positive',
  },
  indigo: {
    border: 'border-l-accent',
    bg: 'bg-accent-soft',
    icon: 'text-accent-strong',
    label: 'text-accent-strong',
    count: 'bg-accent text-paper border-accent',
  },
  amber: {
    border: 'border-l-caution',
    bg: 'bg-caution-soft',
    icon: 'text-caution-strong',
    label: 'text-caution-strong',
    count: 'bg-caution text-paper border-caution',
  },
  rose: {
    border: 'border-l-critical',
    bg: 'bg-critical-soft',
    icon: 'text-critical-strong',
    label: 'text-critical-strong',
    count: 'bg-critical text-paper border-critical',
  },
  fuchsia: {
    border: 'border-l-accent',
    bg: 'bg-accent-soft',
    icon: 'text-accent-strong',
    label: 'text-accent-strong',
    count: 'bg-accent text-paper border-accent',
  },
  slate: {
    border: 'border-l-rule',
    bg: 'bg-paper-2',
    icon: 'text-ink-2',
    label: 'text-ink-2',
    count: 'bg-paper-3 text-ink border-rule',
  },
};

interface Props {
  icon: React.ReactNode;
  label: React.ReactNode;
  count?: number | string | React.ReactNode;
  meta?: React.ReactNode;
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
  const labelText = typeof label === 'string' ? label : '';
  const sizeBytes = typeof count === 'number' && labelText.toLowerCase().includes('document') ? count : null;
  const displayCount =
    typeof count === 'number' && sizeBytes !== null ? fmtSize(count) : count;
  return (
    <section
      className={[
        'relative rounded-md border border-rule/70 border-l-4',
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
          <span className="ml-auto truncate text-xs font-mono text-mute">
            {meta}
          </span>
        )}
      </header>
      <div className={dense ? '' : ''}>{children}</div>
    </section>
  );
}