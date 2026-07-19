import 'server-only';
import { SECONDARY_BCP47, type SecondaryLocale } from '@/i18n/config';

export function formatMoneyServer(
  amount: number | string | null | undefined,
  locale: SecondaryLocale = 'th',
  currency = 'THB',
): string {
  if (amount == null) return '—';
  const n = typeof amount === 'string' ? parseFloat(amount) : amount;
  if (!Number.isFinite(n)) return String(amount);
  const num = new Intl.NumberFormat(SECONDARY_BCP47[locale], {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(n);
  return `${num} ${currency}`;
}

export function formatDateServer(
  d: Date | string | null | undefined,
  locale: SecondaryLocale = 'th',
): string {
  if (!d) return '—';
  const date = typeof d === 'string' ? new Date(d) : d;
  if (!(date instanceof Date) || Number.isNaN(date.getTime())) return '—';
  return new Intl.DateTimeFormat(SECONDARY_BCP47[locale], {
    day: '2-digit',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  }).format(date);
}

export function formatDateShortServer(
  d: Date | string | null | undefined,
  locale: SecondaryLocale = 'th',
): string {
  if (!d) return '—';
  const date = typeof d === 'string' ? new Date(d) : d;
  if (!(date instanceof Date) || Number.isNaN(date.getTime())) return '—';
  return new Intl.DateTimeFormat(SECONDARY_BCP47[locale], {
    day: '2-digit',
    month: 'short',
    year: 'numeric',
  }).format(date);
}
