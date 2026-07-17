import type { ReactNode } from 'react';
import { Icon, type IconName } from '@/components/icons';
import { Panel } from './Panel';

export type AlertTone =
  | 'positive'
  | 'caution'
  | 'critical'
  | 'info'
  | 'neutral'
  | 'accent';

export interface AlertProps {
  tone?: AlertTone;
  title: ReactNode;
  children?: ReactNode;
  actions?: ReactNode;
  onDismiss?: () => void;
  className?: string;
}

const BORDER: Record<AlertTone, string> = {
  positive: 'border-t-positive',
  caution: 'border-t-caution',
  critical: 'border-t-critical',
  info: 'border-t-info',
  neutral: 'border-t-neutral',
  accent: 'border-t-accent',
};

const TEXT: Record<AlertTone, string> = {
  positive: 'text-positive',
  caution: 'text-caution',
  critical: 'text-critical',
  info: 'text-info',
  neutral: 'text-neutral',
  accent: 'text-accent',
};

const ICON: Record<AlertTone, IconName> = {
  positive: 'check-circle',
  caution: 'alert',
  critical: 'alert-circle',
  info: 'info',
  neutral: 'info',
  accent: 'info',
};

export function Alert({
  tone = 'info',
  title,
  children,
  actions,
  onDismiss,
  className = '',
}: AlertProps) {
  return (
    <Panel
      padding="sm"
      className={['border-t-2', BORDER[tone], className].join(' ')}
    >
      <div role="alert" className="flex items-start gap-3">
        <Icon name={ICON[tone]} size={18} className={['mt-0.5 shrink-0', TEXT[tone]].join(' ')} />
        <div className="min-w-0 flex-1">
          <div className="flex items-start justify-between gap-3">
            <div className="text-sm font-semibold text-ink">{title}</div>
            {onDismiss && (
              <button
                type="button"
                onClick={onDismiss}
                aria-label="Dismiss"
                className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded text-mute hover:bg-paper-3 hover:text-ink"
              >
                <Icon name="x" size={14} />
              </button>
            )}
          </div>
          {children && <div className="mt-1 text-sm text-ink-2">{children}</div>}
          {actions && <div className="mt-3 flex flex-wrap items-center gap-2">{actions}</div>}
        </div>
      </div>
    </Panel>
  );
}

export default Alert;
