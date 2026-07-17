import type { ReactNode } from 'react';
import Link from 'next/link';
import { Icon, type IconName } from '@/components/icons';

export interface EmptyAction {
  label: ReactNode;
  onClick?: () => void;
  href?: string;
}

export interface EmptyProps {
  icon?: IconName;
  title: ReactNode;
  body?: ReactNode;
  action?: EmptyAction;
  className?: string;
}

const ACTION = 'inline-flex h-9 items-center justify-center rounded-md border border-rule bg-paper-3 px-4 text-sm font-medium text-ink hover:bg-paper';

export function Empty({
  icon = 'inbox',
  title,
  body,
  action,
  className = '',
}: EmptyProps) {
  return (
    <div className={['mx-auto max-w-md p-10 text-center', className].join(' ')}>
      <Icon name={icon} size={48} strokeWidth={1.5} className="mx-auto text-mute" />
      <div className="mt-4 text-lg font-semibold text-ink">{title}</div>
      {body && <div className="mt-2 text-sm text-ink-2">{body}</div>}
      {action && (
        <div className="mt-5">
          {action.href ? (
            <Link href={action.href} onClick={action.onClick} className={ACTION}>{action.label}</Link>
          ) : (
            <button type="button" onClick={action.onClick} className={ACTION}>{action.label}</button>
          )}
        </div>
      )}
    </div>
  );
}

export default Empty;
