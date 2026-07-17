'use client';

import { useState } from 'react';
import { Badge, Icon, Panel, Status, type BadgeTone, type IconName, type StatusTone } from '@/components/ui';

export interface ToolCardProps {
  name: string;
  status: 'running' | 'ok' | 'empty' | 'error';
  args?: Record<string, unknown>;
  result?: { rows?: unknown[]; rowCount?: number; columns?: string[] };
  sql?: string;
  explanation?: string;
  error?: string;
}

const META: Record<ToolCardProps['status'], { icon: IconName; tone: StatusTone; badge: BadgeTone; label: string }> = {
  running: { icon: 'loader', tone: 'caution', badge: 'caution', label: 'running' },
  ok: { icon: 'check-circle', tone: 'positive', badge: 'positive', label: 'ok' },
  empty: { icon: 'minus', tone: 'neutral', badge: 'neutral', label: 'empty' },
  error: { icon: 'alert-circle', tone: 'critical', badge: 'critical', label: 'error' },
};

function fmt(value: unknown) {
  if (value == null) return '—';
  if (typeof value === 'string') return value;
  try { return JSON.stringify(value); } catch { return String(value); }
}

export function ToolCard(props: ToolCardProps) {
  const [open, setOpen] = useState(false);
  const meta = META[props.status];
  const args = Object.entries(props.args ?? {}).filter(([, value]) => value != null && value !== '');
  const rows = props.result?.rowCount ?? props.result?.rows?.length ?? 0;
  const details = args.length > 0 || props.sql || props.explanation || props.error || rows > 0;

  return (
    <Panel padding="none" className="my-1 overflow-hidden">
      <button type="button" onClick={() => setOpen((value) => !value)} aria-expanded={open} className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs hover:bg-paper-3">
        <Icon name={meta.icon} size={15} className={props.status === 'running' ? 'animate-spin text-caution' : ''} />
        <Badge tone={meta.badge} size="sm">{props.name}</Badge>
        <Status tone={meta.tone} size="sm">{meta.label}</Status>
        {props.result && <span className="text-mute">· {rows} {rows === 1 ? 'row' : 'rows'}</span>}
        <Icon name="chevron-right" size={14} className={['ml-auto transition-transform', open ? 'rotate-90' : ''].join(' ')} />
      </button>
      {open && details && (
        <div className="space-y-2 border-t border-rule bg-paper px-3 py-2 font-mono text-xs text-ink-2">
          {args.length > 0 && <div><div className="mb-1 text-mute">args:</div><div className="space-y-1 pl-2">{args.map(([key, value]) => <div key={key} className="flex gap-2"><span className="text-mute">{key}:</span><span className="break-all">{fmt(value)}</span></div>)}</div></div>}
          {props.sql && <div><div className="mb-1 text-mute">sql:</div><pre className="overflow-x-auto whitespace-pre-wrap rounded-md border border-rule bg-paper-2 p-2 text-xs">{props.sql}</pre></div>}
          {props.explanation && <div className="font-sans italic text-mute">{props.explanation}</div>}
          {props.error && <Status tone="critical"><Icon name="alert" size={14} />{props.error}</Status>}
          {rows > 0 && props.result?.rows?.[0] != null && <div><div className="mb-1 text-mute">first row:</div><pre className="overflow-x-auto whitespace-pre-wrap rounded-md border border-rule bg-paper-2 p-2 text-xs">{JSON.stringify(props.result.rows[0])}</pre></div>}
        </div>
      )}
    </Panel>
  );
}
