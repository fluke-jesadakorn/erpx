'use client';

import { useState } from 'react';
import { Badge, Icon, Panel } from '@/components/ui';

export interface ThinkingStepsProps {
  steps: Array<{ text: string }>;
}

export function ThinkingSteps({ steps }: ThinkingStepsProps) {
  const [open, setOpen] = useState(false);
  if (!steps?.length) return null;

  return (
    <Panel padding="none" className="my-1 overflow-hidden">
      <button type="button" onClick={() => setOpen((value) => !value)} aria-expanded={open} className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-ink-2 hover:bg-paper-3">
        <Icon name="cpu" size={15} className="text-accent" />
        <Badge tone="accent" size="sm">Thinking</Badge>
        <span className="text-mute">{steps.length} {steps.length === 1 ? 'step' : 'steps'}</span>
        <Icon name="chevron-right" size={14} className={['ml-auto transition-transform', open ? 'rotate-90' : ''].join(' ')} />
      </button>
      {open && <div className="border-t border-rule bg-paper px-3 py-2 font-mono text-xs leading-relaxed text-ink-2"><ol className="list-decimal space-y-1 pl-4">{steps.map((step, i) => <li key={i}>{step.text}</li>)}</ol></div>}
    </Panel>
  );
}
