import type { ReactNode } from 'react';

export interface KbdProps {
  children: ReactNode;
  className?: string;
}

export function Kbd({ children, className = '' }: KbdProps) {
  return (
    <kbd
      className={[
        'inline-flex min-w-[22px] h-[22px] items-center justify-center rounded border border-rule bg-paper-3 px-1.5 text-xs font-mono text-mute',
        className,
      ].join(' ')}
    >
      {children}
    </kbd>
  );
}

export default Kbd;
