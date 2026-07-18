import type { ReactNode } from 'react';
import Link from 'next/link';
import { Inbox, type LucideIcon } from 'lucide-react';

export interface EmptyAction {
  label: ReactNode;
  onClick?: () => void;
  href?: string;
}

export interface EmptyProps {
  icon?: LucideIcon;
  title: ReactNode;
  body?: ReactNode;
  action?: EmptyAction;
  className?: string;
}

const ACTION = 'glass-input inline-flex h-10 items-center justify-center px-4 text-sm font-medium text-ink transition-all hover:-translate-y-px hover:border-rule-strong';

export function Empty({
  icon: IconCmp = Inbox,
  title,
  body,
  action,
  className = '',
}: EmptyProps) {
  return (
    <div className={['mx-auto max-w-md px-6 py-12 text-center', className].join(' ')}>
      <span className="panel mx-auto flex h-16 w-16 items-center justify-center rounded-2xl text-accent">
        <IconCmp size={28} strokeWidth={1.6} />
      </span>
      <div className="mt-5 text-lg font-semibold tracking-tight text-ink">{title}</div>
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
