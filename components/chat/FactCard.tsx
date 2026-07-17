'use client';

import { Alert, Badge, Panel } from '@/components/ui';

export interface FactCardProps {
  rows: Array<Record<string, unknown>>;
  columns: string[];
  explanation?: string;
}

function value(row: Record<string, unknown>, ...keys: string[]) {
  for (const key of keys) {
    const item = row[key] ?? row[key.toLowerCase()] ?? row[key.toUpperCase()];
    if (item != null && item !== '') return String(item);
  }
  return null;
}

export function FactCard({ rows, columns, explanation }: FactCardProps) {
  if (!rows?.length) {
    return explanation ? <Alert tone="info" title="Query result">{explanation.replace(/^\[(runtime error|rejected|no sql returned)\]\s*/i, '')}</Alert> : null;
  }
  if (rows.length > 1) return null;

  const row = rows[0];
  const name = value(row, 'fullname', 'name', 'display_name');
  const code = value(row, 'employee_code', 'code', 'doc_no');
  const id = value(row, 'id');
  const extra = columns
    .filter((col) => !['fullname', 'name', 'display_name', 'employee_code', 'code', 'doc_no', 'id'].includes(col.toLowerCase()))
    .map((col) => ({ label: col, value: value(row, col) }))
    .filter((item) => item.value);

  return (
    <Panel padding="sm" className="my-1 inline-flex flex-wrap items-center gap-2 border-accent/40 bg-accent-soft">
      {name && <span className="text-sm font-semibold text-ink">{name}</span>}
      {code && <Badge tone="accent">{code}</Badge>}
      {id && !code && <Badge tone="neutral">id {id}</Badge>}
      {extra.length > 0 && <span className="ml-2 flex flex-wrap gap-x-3 gap-y-1 text-xs text-mute">{extra.map((item) => <span key={item.label} className="font-mono"><span className="text-ink-2">{item.label}:</span> {item.value}</span>)}</span>}
    </Panel>
  );
}
