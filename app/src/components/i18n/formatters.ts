'use client';

import { useSecondaryLocale } from './SecondaryLocaleProvider';

export function useFormatMoney() {
  const locale = useSecondaryLocale();
  return (amount: number | string | null | undefined, currency = 'THB'): string => {
    if (amount == null) return '—';
    const n = typeof amount === 'string' ? parseFloat(amount) : amount;
    if (!Number.isFinite(n)) return String(amount);
    const intl = new Intl.NumberFormat(locale === 'de' ? 'de-DE' : 'th-TH', {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
    return `${intl.format(n)} ${currency}`;
  };
}

export function useFormatDate() {
  const locale = useSecondaryLocale();
  return (d: Date | string | null | undefined): string => {
    if (!d) return '—';
    const date = typeof d === 'string' ? new Date(d) : d;
    if (!(date instanceof Date) || Number.isNaN(date.getTime())) return '—';
    return new Intl.DateTimeFormat(locale === 'de' ? 'de-DE' : 'th-TH', {
      day: '2-digit',
      month: 'short',
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    }).format(date);
  };
}