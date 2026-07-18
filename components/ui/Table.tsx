'use client';
import React from 'react';
import { Panel } from './Panel';
import { Skeleton } from './Skeleton';

export interface Column<T> {
  key: string;
  header: React.ReactNode;
  render: (row: T, index: number) => React.ReactNode;
  align?: 'left' | 'right' | 'center';
  className?: string;
  width?: string;
  thClassName?: string;
}

export interface TableProps<T> {
  columns: Column<T>[];
  rows: T[];
  rowKey: (row: T, index: number) => string;
  onRowClick?: (row: T) => void;
  emptyState?: React.ReactNode;
  loading?: boolean;
  zebra?: boolean;
  className?: string;
  size?: 'sm' | 'md';
  stickyHeader?: boolean;
  scrollable?: boolean;
}

const ALIGN: Record<NonNullable<Column<unknown>['align']>, string> = {
  left: 'text-left',
  right: 'text-right',
  center: 'text-center',
};

export function Table<T>({
  columns,
  rows,
  rowKey,
  onRowClick,
  emptyState,
  loading = false,
  zebra = false,
  className = '',
  size = 'md',
  stickyHeader = false,
  scrollable = true,
}: TableProps<T>) {
  const cellPad = size === 'sm' ? 'px-3 py-2 text-xs' : 'px-4 py-3 text-sm';
  const head = stickyHeader ? 'sticky top-0 z-dropdown' : '';

  if (loading) {
    return (
      <Panel padding="none" className={className}>
        <div className={scrollable ? 'overflow-x-auto' : ''}>
        <table className="w-full min-w-[42rem]">
          <thead>
            <tr className={`border-b border-rule bg-paper-2/90 backdrop-blur-xl ${head}`}>
              {columns.map((col) => (
                <th
                  key={col.key}
                  className={[
                    'px-4 py-2.5 text-[11px] font-medium uppercase tracking-[0.12em] text-mute',
                    ALIGN[col.align ?? 'left'],
                    col.thClassName,
                  ].join(' ')}
                >
                  {col.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {Array.from({ length: 5 }).map((_, i) => (
              <tr key={i} className="border-b border-rule">
                {columns.map((col) => (
                  <td key={col.key} className={cellPad}>
                    <Skeleton className="h-4 w-3/4" />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        </div>
      </Panel>
    );
  }

  if (rows.length === 0 && emptyState) {
    return (
      <Panel padding="none" className={className}>
        <div className="p-6">{emptyState}</div>
      </Panel>
    );
  }

  return (
    <Panel padding="none" className={className}>
      <div className={scrollable ? 'overflow-x-auto' : ''}>
      <table className="w-full min-w-[42rem]">
        <thead>
          <tr className={`border-b border-rule bg-paper-2/90 backdrop-blur-xl ${head}`}>
            {columns.map((col) => (
              <th
                key={col.key}
                style={col.width ? { width: col.width } : undefined}
                className={[
                  'px-4 py-2.5 text-[11px] font-medium uppercase tracking-[0.12em] text-mute',
                  ALIGN[col.align ?? 'left'],
                  col.thClassName,
                ].join(' ')}
              >
                {col.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-rule">
          {rows.map((row, idx) => {
            const clickable = !!onRowClick;
            return (
              <tr
                key={rowKey(row, idx)}
                onClick={clickable ? () => onRowClick!(row) : undefined}
                className={[
                  clickable ? 'cursor-pointer hover:bg-paper-3/55' : '',
                  zebra && idx % 2 === 1 ? 'bg-paper-2/40' : '',
                ]
                  .join(' ')
                  .trim()}
              >
                {columns.map((col) => (
                  <td
                    key={col.key}
                    className={[cellPad, ALIGN[col.align ?? 'left'], 'text-ink', col.className].join(' ')}
                  >
                    {col.render(row, idx)}
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
      </div>
    </Panel>
  );
}

export default Table;
