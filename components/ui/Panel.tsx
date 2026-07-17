import type { ReactNode } from 'react';

export type PanelTone = 'default' | 'elevated';
export type PanelPad = 'none' | 'sm' | 'md' | 'lg';

export interface PanelProps {
  tone?: PanelTone;
  padding?: PanelPad;
  className?: string;
  children: ReactNode;
}

const PAD: Record<PanelPad, string> = {
  none: 'p-0',
  sm: 'p-4',
  md: 'p-6',
  lg: 'p-8',
};

export function Panel({
  tone = 'default',
  padding = 'md',
  className = '',
  children,
}: PanelProps) {
  const base = tone === 'elevated' ? 'panel-elevated' : 'panel';
  return <div className={[base, PAD[padding], className].join(' ')}>{children}</div>;
}

export default Panel;
