'use client';

import { useFormatter } from 'next-intl';

export function useFormatMoney() {
  const fmt = useFormatter();
  return (amount: number | string | null | undefined, currency = 'THB'): string => {
    if (amount == null) return '—';
    const n = typeof amount === 'string' ? parseFloat(amount) : amount;
    if (!Number.isFinite(n)) return String(amount);
    const num = fmt.number(n, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
    return `${num} ${currency}`;
  };
}

export function useFormatDate() {
  const fmt = useFormatter();
  return (d: Date | string | null | undefined): string => {
    if (!d) return '—';
    const date = typeof d === 'string' ? new Date(d) : d;
    if (!(date instanceof Date) || Number.isNaN(date.getTime())) return '—';
    return fmt.dateTime(date, {
      day: '2-digit',
      month: 'short',
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    });
  };
}
