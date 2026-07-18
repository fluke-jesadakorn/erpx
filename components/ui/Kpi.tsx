import React from 'react';

export type KpiTone =
  | 'positive'
  | 'caution'
  | 'critical'
  | 'info'
  | 'accent'
  | 'neutral'
  | 'default';

const TONE: Record<KpiTone, { text: string; border: string }> = {
  positive: { text: 'text-positive', border: 'border-positive/40' },
  caution: { text: 'text-caution', border: 'border-caution/40' },
  critical: { text: 'text-critical', border: 'border-critical/40' },
  info: { text: 'text-info', border: 'border-info/40' },
  accent: { text: 'text-accent', border: 'border-accent/40' },
  neutral: { text: 'text-neutral', border: 'border-neutral/40' },
  default: { text: 'text-ink', border: 'border-rule' },
};

export interface KpiProps {
  label: React.ReactNode;
  value: React.ReactNode;
  tone?: KpiTone;
  size?: 'sm' | 'md';
  caption?: React.ReactNode;
  valueClassName?: string;
}

export const Kpi: React.FC<KpiProps> = ({ label, value, tone = 'default', size = 'md', caption, valueClassName }) => {
  const t = TONE[tone];
  const padding = size === 'sm' ? 'p-3' : 'p-4';
  const textSize = size === 'sm' ? 'text-xl' : 'text-2xl';
  return (
    <div className={`panel relative overflow-hidden ${padding} ${t.border}`}>
      <span aria-hidden className={`absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-current to-transparent ${t.text} opacity-65`} />
      <span className="block text-[11px] font-medium uppercase tracking-[0.14em] text-mute">
        {label}
      </span>
      <span className={`${textSize} num-tabular mt-2 block font-mono font-semibold tracking-[-0.045em] ${valueClassName ?? t.text}`}>
        {value}
      </span>
      {caption && (
        <span className="mt-1 block text-xs text-mute">{caption}</span>
      )}
    </div>
  );
};

export default Kpi;
