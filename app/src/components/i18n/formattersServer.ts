import 'server-only';

export function formatMoneyServer(
  amount: number | string | null | undefined,
  locale: 'th' | 'de',
  currency = 'THB',
): string {
  if (amount == null) return '—';
  const n = typeof amount === 'string' ? parseFloat(amount) : amount;
  if (!Number.isFinite(n)) return String(amount);
  const intl = new Intl.NumberFormat(locale === 'de' ? 'de-DE' : 'th-TH', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  return `${intl.format(n)} ${currency}`;
}

export function formatDateServer(
  d: Date | string | null | undefined,
  locale: 'th' | 'de',
): string {
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
}