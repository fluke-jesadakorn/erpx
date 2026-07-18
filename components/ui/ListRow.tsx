import React from 'react';

export interface ListRowProps {
  children: React.ReactNode;
  onClick?: () => void;
  href?: string;
  className?: string;
}

export function ListRow({ children, onClick, href, className = '' }: ListRowProps) {
  const base = 'flex min-h-14 items-center gap-3 border-b border-rule/75 px-4 py-3.5 last:border-b-0 transition-colors duration-[var(--dur-base)] hover:bg-paper-3/45';
  if (href) {
    return <a href={href} className={[base, 'cursor-pointer', className].join(' ')}>{children}</a>;
  }
  if (onClick) {
    return <button type="button" onClick={onClick} className={[base, 'cursor-pointer text-left w-full', className].join(' ')}>{children}</button>;
  }
  return <div className={[base, className].join(' ')}>{children}</div>;
}

export default ListRow;
