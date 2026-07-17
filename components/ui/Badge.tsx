import type { HTMLAttributes, ReactNode } from 'react';

export type BadgeTone =
  | 'positive'
  | 'caution'
  | 'critical'
  | 'info'
  | 'neutral'
  | 'accent';

export interface BadgeProps extends Omit<HTMLAttributes<HTMLSpanElement>, 'children'> {
  tone?: BadgeTone;
  dot?: boolean;
  size?: 'sm' | 'md';
  className?: string;
  children: ReactNode;
}

const TONE: Record<BadgeTone, string> = {
  positive: 'bg-positive-soft text-positive border-positive/40',
  caution: 'bg-caution-soft text-caution border-caution/40',
  critical: 'bg-critical-soft text-critical border-critical/40',
  info: 'bg-info-soft text-info border-info/40',
  neutral: 'bg-paper-3 text-ink-2 border-rule',
  accent: 'bg-accent-soft text-accent border-accent/40',
};

const DOT: Record<BadgeTone, string> = {
  positive: 'bg-positive',
  caution: 'bg-caution',
  critical: 'bg-critical',
  info: 'bg-info',
  neutral: 'bg-neutral',
  accent: 'bg-accent',
};

const SIZE = {
  sm: 'h-5 px-1.5 text-xs',
  md: 'h-6 px-2 text-xs',
};

export function Badge({
  tone = 'neutral',
  dot = false,
  size = 'md',
  className = '',
  children,
  ...rest
}: BadgeProps) {
  return (
    <span
      {...rest}
      className={[
        'inline-flex items-center gap-1.5 rounded-full border font-medium tabular-nums whitespace-nowrap',
        SIZE[size],
        TONE[tone],
        className,
      ].join(' ')}
    >
      {dot && <span aria-hidden className={['h-1.5 w-1.5 rounded-full', DOT[tone]].join(' ')} />}
      {children}
    </span>
  );
}

export default Badge;
