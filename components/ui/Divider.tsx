import type { ReactNode } from 'react';

export interface DividerProps {
  label?: ReactNode;
  className?: string;
}

export function Divider({ label, className = '' }: DividerProps) {
  if (!label) return <hr className={['my-4 border-0 border-t border-rule', className].join(' ')} />;

  return (
    <div className={['my-4 flex items-center', className].join(' ')}>
      <span aria-hidden className="flex-1 border-t border-rule" />
      <span className="px-3 text-xs uppercase tracking-wider text-mute">{label}</span>
      <span aria-hidden className="flex-1 border-t border-rule" />
    </div>
  );
}

export default Divider;
