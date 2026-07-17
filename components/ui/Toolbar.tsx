import type { ReactNode } from 'react';

export interface ToolbarProps {
  children: ReactNode;
  sticky?: boolean;
  className?: string;
}

export function Toolbar({
  children,
  sticky = true,
  className = '',
}: ToolbarProps) {
  return (
    <div
      className={[
        sticky ? 'sticky top-14 z-20' : '',
        'border-b border-rule bg-paper-2 px-4 py-2.5 sm:px-6',
        className,
      ].join(' ')}
    >
      <div className="flex flex-wrap items-center gap-2">{children}</div>
    </div>
  );
}

export default Toolbar;
