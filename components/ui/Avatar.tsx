export type AvatarSize = 20 | 24 | 32 | 40 | 56;
export type AvatarTone = 'neutral' | 'accent';
export type AvatarStatus = 'positive' | 'caution' | 'critical' | 'info' | 'neutral';

export interface AvatarProps {
  name: string;
  size?: AvatarSize;
  tone?: AvatarTone;
  status?: AvatarStatus;
  className?: string;
}

const SIZE: Record<AvatarSize, string> = {
  20: 'h-5 w-5 text-[10px]',
  24: 'h-6 w-6 text-xs',
  32: 'h-8 w-8 text-sm',
  40: 'h-10 w-10 text-base',
  56: 'h-14 w-14 text-lg',
};

const TONE: Record<AvatarTone, string> = {
  neutral: 'border-rule bg-paper-3 text-ink',
  accent: 'border-accent/40 bg-accent-soft text-accent',
};

const STATUS: Record<AvatarStatus, string> = {
  positive: 'bg-positive',
  caution: 'bg-caution',
  critical: 'bg-critical',
  info: 'bg-info',
  neutral: 'bg-neutral',
};

function initials(name: string) {
  const parts = name.trim().split(/\s+/).filter(Boolean).slice(0, 2);
  if (parts.length === 0) return '?';
  return parts.map((part) => part[0]).join('').toUpperCase();
}

export function Avatar({
  name,
  size = 32,
  tone = 'neutral',
  status,
  className = '',
}: AvatarProps) {
  return (
    <span
      role="img"
      aria-label={name}
      className={['relative inline-flex shrink-0 select-none', className].join(' ')}
    >
      <span
        className={[
          'inline-flex items-center justify-center rounded-full border font-medium',
          SIZE[size],
          TONE[tone],
        ].join(' ')}
      >
        {initials(name)}
      </span>
      {status && (
        <span
          aria-hidden
          className={[
            'absolute bottom-0 right-0 h-2 w-2 rounded-full border border-paper',
            STATUS[status],
          ].join(' ')}
        />
      )}
    </span>
  );
}

export default Avatar;
