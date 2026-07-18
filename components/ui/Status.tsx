import type { ReactNode } from 'react';

export type StatusTone =
  | 'positive'
  | 'caution'
  | 'critical'
  | 'info'
  | 'neutral'
  | 'accent';

export interface StatusProps {
  tone?: StatusTone;
  size?: 'sm' | 'md';
  dot?: boolean;
  className?: string;
  children: ReactNode;
}

const TEXT: Record<StatusTone, string> = {
  positive: 'text-positive',
  caution: 'text-caution',
  critical: 'text-critical',
  info: 'text-info',
  neutral: 'text-ink-2',
  accent: 'text-accent',
};

const DOT: Record<StatusTone, string> = {
  positive: 'bg-positive',
  caution: 'bg-caution',
  critical: 'bg-critical',
  info: 'bg-info',
  neutral: 'bg-neutral',
  accent: 'bg-accent',
};

export function Status({
  tone = 'neutral',
  size = 'md',
  dot = true,
  className = '',
  children,
}: StatusProps) {
  return (
    <span
      className={[
        'inline-flex items-center gap-1.5 font-medium tracking-[-0.01em]',
        size === 'sm' ? 'text-xs' : 'text-sm',
        TEXT[tone],
        className,
      ].join(' ')}
    >
      {dot && (
        <span
          aria-hidden
          className={[
            'rounded-full ring-2 ring-current/15',
            size === 'sm' ? 'h-1.5 w-1.5' : 'h-2 w-2',
            DOT[tone],
          ].join(' ')}
        />
      )}
      {children}
    </span>
  );
}

export default Status;
