'use client';

import { useState } from 'react';
import { ChevronRight } from 'lucide-react';
import type { UiNode, UiTabs, UiAccordion, UiButton, UiTable, UiMetric, UiNote, UiCard, UiStack, UiGrid, UiHeading, UiText, UiRoot } from '@folio-lib/ai/safeUiContract';
import { resolveAction } from '@folio-lib/chat/safeUiActions';
import { Alert, Kpi, Panel, Tabs } from '@/components/ui';

function action(value: string | undefined, onAction?: (name: string, payload?: Record<string, unknown>) => void) {
  return (event: React.MouseEvent) => {
    event.preventDefault();
    if (!value) return;
    const spec = resolveAction(value);
    if (!spec) return;
    if (spec.kind === 'navigate' && spec.href) window.location.assign(spec.href);
    else if (spec.kind === 'copy' && spec.apiPath) void navigator.clipboard.writeText(spec.apiPath);
    else if (spec.kind === 'api' && spec.apiPath) void fetch(spec.apiPath, { method: spec.method ?? 'GET' });
    onAction?.(value);
  };
}

function cls(...values: Array<string | undefined | false>) {
  return values.filter(Boolean).join(' ');
}

function RootView({ node, onAction }: { node: UiRoot; onAction?: (name: string, payload?: Record<string, unknown>) => void }) {
  return <div className="space-y-3">{node.children.map((child, i) => <NodeRenderer key={i} node={child} onAction={onAction} />)}</div>;
}

function StackView({ node, onAction }: { node: UiStack; onAction?: (name: string, payload?: Record<string, unknown>) => void }) {
  return <div className={cls('flex', node.direction === 'row' ? 'flex-row flex-wrap items-center' : 'flex-col')} style={{ gap: `${node.gap ?? 8}px` }}>{node.children.map((child, i) => <NodeRenderer key={i} node={child} onAction={onAction} />)}</div>;
}

function GridView({ node, onAction }: { node: UiGrid; onAction?: (name: string, payload?: Record<string, unknown>) => void }) {
  return <div className="grid" style={{ gridTemplateColumns: `repeat(${Math.min(Math.max(node.columns ?? 2, 1), 6)}, minmax(0, 1fr))`, gap: `${node.gap ?? 8}px` }}>{node.children.map((child, i) => <NodeRenderer key={i} node={child} onAction={onAction} />)}</div>;
}

function CardView({ node, onAction }: { node: UiCard; onAction?: (name: string, payload?: Record<string, unknown>) => void }) {
  return <Panel padding="sm" className="bg-paper">{node.title && <div className="mb-2 text-xs font-medium uppercase tracking-wider text-mute">{node.title}</div>}<div className="space-y-2">{node.children.map((child, i) => <NodeRenderer key={i} node={child} onAction={onAction} />)}</div></Panel>;
}

function HeadingView({ node }: { node: UiHeading }) {
  const style = 'text-sm font-semibold text-ink';
  if (node.level === 1) return <h1 className={style}>{node.text}</h1>;
  if (node.level === 3) return <h3 className={style}>{node.text}</h3>;
  return <h2 className={style}>{node.text}</h2>;
}

function TextView({ node }: { node: UiText }) {
  return <div className="whitespace-pre-wrap break-words text-sm text-ink-2">{node.text}</div>;
}

function MetricView({ node }: { node: UiMetric }) {
  return <Kpi label={node.label} value={node.value} caption={node.hint} />;
}

function TableView({ node }: { node: UiTable }) {
  return <Panel padding="none" className="overflow-x-auto bg-paper"><table className="w-full text-xs">{node.caption && <caption className="bg-paper-3 px-2 py-2 text-left font-mono text-mute">{node.caption}</caption>}<thead className="bg-paper-3 text-mute"><tr>{node.columns.map((col, i) => <th key={i} className="px-2 py-2 text-left font-medium uppercase tracking-wide">{col}</th>)}</tr></thead><tbody>{node.rows.length === 0 ? <tr><td colSpan={Math.max(node.columns.length, 1)} className="px-2 py-3 text-center text-mute">No rows</td></tr> : node.rows.map((row, ri) => <tr key={ri} className="border-t border-rule hover:bg-paper-3">{row.map((cell, ci) => <td key={ci} className="px-2 py-2 text-ink-2">{String(cell ?? '')}</td>)}</tr>)}</tbody></table></Panel>;
}

function TabsView({ node, onAction }: { node: UiTabs; onAction?: (name: string, payload?: Record<string, unknown>) => void }) {
  const [active, setActive] = useState(node.active ?? node.tabs[0]?.id ?? '');
  const current = node.tabs.find((tab) => tab.id === active) ?? node.tabs[0];
  return <Panel padding="sm" className="bg-paper"><Tabs value={active} onValueChange={setActive} items={node.tabs.map((tab) => ({ value: tab.id, label: tab.label }))} className="mb-3" /><div className="space-y-2">{current?.children.map((child, i) => <NodeRenderer key={i} node={child} onAction={onAction} />)}</div></Panel>;
}

function AccordionView({ node, onAction }: { node: UiAccordion; onAction?: (name: string, payload?: Record<string, unknown>) => void }) {
  const [open, setOpen] = useState<Record<string, boolean>>({});
  return <Panel padding="none" className="divide-y divide-rule overflow-hidden">{node.items.map((item) => <div key={item.id}><button type="button" onClick={() => setOpen((value) => ({ ...value, [item.id]: !value[item.id] }))} className="flex w-full items-center justify-between px-3 py-2 text-left text-sm text-ink hover:bg-paper-3" aria-expanded={!!open[item.id]}><span>{item.title}</span><ChevronRight size={14} className={open[item.id] ? 'rotate-90' : ''} /></button>{open[item.id] && <div className="space-y-2 px-3 pb-3">{item.children.map((child, i) => <NodeRenderer key={i} node={child} onAction={onAction} />)}</div>}</div>)}</Panel>;
}

function ButtonView({ node, onAction }: { node: UiButton; onAction?: (name: string, payload?: Record<string, unknown>) => void }) {
  const style = node.variant === 'primary' ? 'bg-accent text-accent-ink hover:bg-accent-strong' : node.variant === 'danger' ? 'bg-critical text-paper hover:bg-critical-strong' : 'border border-rule bg-paper text-ink-2 hover:bg-paper-3';
  return <button type="button" onClick={action(node.action, onAction)} className={cls('h-9 rounded-md px-3 text-xs font-medium', style)}>{node.label}</button>;
}

function NoteView({ node }: { node: UiNote }) {
  return <Alert tone={node.tone === 'warn' ? 'caution' : node.tone === 'danger' ? 'critical' : 'info'} title={node.text} />;
}

function NodeRenderer({ node, onAction }: { node: UiNode; onAction?: (name: string, payload?: Record<string, unknown>) => void }) {
  switch (node.type) {
    case 'root': return <RootView node={node} onAction={onAction} />;
    case 'stack': return <StackView node={node} onAction={onAction} />;
    case 'grid': return <GridView node={node} onAction={onAction} />;
    case 'card': return <CardView node={node} onAction={onAction} />;
    case 'heading': return <HeadingView node={node} />;
    case 'text': return <TextView node={node} />;
    case 'metric': return <MetricView node={node} />;
    case 'table': return <TableView node={node} />;
    case 'tabs': return <TabsView node={node} onAction={onAction} />;
    case 'accordion': return <AccordionView node={node} onAction={onAction} />;
    case 'button': return <ButtonView node={node} onAction={onAction} />;
    case 'note': return <NoteView node={node} />;
  }
}

export function SafeUiRenderer({ root, onAction }: { root: UiNode; onAction?: (name: string, payload?: Record<string, unknown>) => void }) {
  return <div className="folio-safe-ui my-1"><NodeRenderer node={root} onAction={onAction} /></div>;
}
