'use client';

import { useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { Badge } from '@/components/ui';

export interface ModelChipProps {
  modelName?: string | null;
  latencyMs?: number | null;
}

export function ModelChip({ modelName, latencyMs }: ModelChipProps) {
  const [copied, setCopied] = useState(false);
  if (!modelName && latencyMs == null) return null;
  const text = `${modelName ?? ''}${latencyMs != null ? ` · ${latencyMs}ms` : ''}`;
  return (
    <div className="mt-1 flex items-center gap-1">
      <Badge tone="neutral" size="sm">{text}</Badge>
      <button type="button" onClick={() => { void navigator.clipboard.writeText(text); setCopied(true); setTimeout(() => setCopied(false), 1500); }} className="inline-flex h-6 w-6 items-center justify-center rounded text-mute hover:bg-paper-3 hover:text-ink" aria-label="Copy model details">{copied ? <Check size={12} /> : <Copy size={12} />}</button>
    </div>
  );
}
