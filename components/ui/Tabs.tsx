'use client';

import type { ReactNode } from 'react';
import Link from 'next/link';

export type TabsVariant = 'segmented' | 'page';

export interface TabsItem {
  value: string;
  label: ReactNode;
  href?: string;
}

export interface TabsProps {
  value: string;
  onValueChange?: (value: string) => void;
  items: TabsItem[];
  variant?: TabsVariant;
  className?: string;
}

export function Tabs({
  value,
  onValueChange,
  items,
  variant = 'segmented',
  className = '',
}: TabsProps) {
  return (
    <TabsList variant={variant} className={className}>
      {items.map((item) => (
        <TabsTrigger
          key={item.value}
          variant={variant}
          active={item.value === value}
          href={variant === 'page' ? item.href : undefined}
          onClick={() => onValueChange?.(item.value)}
        >
          {item.label}
        </TabsTrigger>
      ))}
    </TabsList>
  );
}

export interface TabsListProps {
  variant?: TabsVariant;
  className?: string;
  children: ReactNode;
}

export function TabsList({
  variant = 'segmented',
  className = '',
  children,
}: TabsListProps) {
  const base = variant === 'segmented'
    ? 'glass-toolbar inline-flex p-1'
    : 'flex border-b border-rule/80';

  return <div className={[base, className].join(' ')}>{children}</div>;
}

export interface TabsTriggerProps {
  variant?: TabsVariant;
  active: boolean;
  href?: string;
  onClick?: () => void;
  className?: string;
  children: ReactNode;
}

export function TabsTrigger({
  variant = 'segmented',
  active,
  href,
  onClick,
  className = '',
  children,
}: TabsTriggerProps) {
  const segmented = [
    'inline-flex h-8 items-center justify-center rounded-md border px-3 text-sm font-medium transition-all duration-[var(--dur-base)]',
    active
      ? 'border-rule-strong bg-paper-3/80 text-ink shadow-sm'
      : 'border-transparent text-ink-2 hover:bg-paper-3/40 hover:text-ink',
  ].join(' ');
  const page = [
    '-mb-px inline-flex h-10 items-center border-b-2 px-3 text-sm font-medium transition-colors',
    active
      ? 'border-accent text-ink'
      : 'border-transparent text-ink-2 hover:text-ink',
  ].join(' ');
  const cls = [variant === 'segmented' ? segmented : page, className].join(' ');

  if (variant === 'page' && href) {
    return <Link href={href} onClick={onClick} className={cls}>{children}</Link>;
  }

  return <button type="button" onClick={onClick} className={cls}>{children}</button>;
}

export interface TabsContentProps {
  value: string;
  current: string;
  className?: string;
  children: ReactNode;
}

export function TabsContent({ value, current, className = '', children }: TabsContentProps) {
  if (value !== current) return null;
  return <div className={className}>{children}</div>;
}

export default Tabs;
