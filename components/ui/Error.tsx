import type { ReactNode } from 'react';
import { Icon } from '@/components/icons';
import { Panel } from './Panel';

export interface ErrorRetry {
  label: ReactNode;
  onClick?: () => void;
}

export interface ErrorProps {
  title?: ReactNode;
  body?: ReactNode;
  retry?: ErrorRetry;
  className?: string;
}

export function Error({
  title = 'Something went wrong',
  body,
  retry,
  className = '',
}: ErrorProps) {
  return (
    <Panel padding="none" className={['border-t-2 border-t-critical p-10', className].join(' ')}>
      <div className="mx-auto max-w-md text-center">
        <Icon name="alert-circle" size={48} strokeWidth={1.5} className="mx-auto text-critical" />
        <div className="mt-4 text-lg font-semibold text-ink">{title}</div>
        {body && <div className="mt-2 text-sm text-ink-2">{body}</div>}
        {retry && (
          <button
            type="button"
            onClick={retry.onClick}
            className="mt-5 inline-flex h-9 items-center justify-center rounded-md border border-rule bg-paper-3 px-4 text-sm font-medium text-ink hover:bg-paper"
          >
            {retry.label}
          </button>
        )}
      </div>
    </Panel>
  );
}

export const ErrorView = Error;

export default Error;
