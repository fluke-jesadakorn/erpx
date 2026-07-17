import 'server-only';

export interface SqlInsight {
  label: string;
  value: string;
  tone?: 'positive' | 'negative' | 'neutral' | 'warning';
}

export interface SqlInsights {
  headline: string;
  bullets: SqlInsight[];
}

type Row = Record<string, unknown>;

function num(v: unknown): number | null {
  if (v == null || v === '') return null;
  if (typeof v === 'number') return Number.isFinite(v) ? v : null;
  const n = Number(v);
  return Number.isFinite(n) ? n : null;
}

function lowerKeys(row: Row): Map<string, unknown> {
  const m = new Map<string, unknown>();
  for (const [k, v] of Object.entries(row)) m.set(k.toLowerCase(), v);
  return m;
}

function thb(n: number): string {
  return n.toLocaleString('th-TH', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

function statusLabel(s: string): string {
  return s.replace(/_/g, ' ');
}

function pct(part: number, total: number): string {
  if (!total) return '0%';
  return `${Math.round((part / total) * 100)}%`;
}

function detectExpenseList(columns: string[]): boolean {
  const c = new Set(columns.map((x) => x.toLowerCase()));
  return c.has('total_amount') && (c.has('status') || c.has('vendor_name'));
}

function detectExpenseItems(columns: string[]): boolean {
  const c = new Set(columns.map((x) => x.toLowerCase()));
  return c.has('amount') && (c.has('category_name') || c.has('mapped_account_code') || c.has('item_description'));
}

function detectVendorAgg(columns: string[]): boolean {
  const c = new Set(columns.map((x) => x.toLowerCase()));
  return c.has('vendor_name') && (c.has('total') || c.has('total_amount') || c.has('sum'));
}

function detectScalarSum(columns: string[]): boolean {
  if (columns.length !== 1) return false;
  const c = columns[0].toLowerCase();
  return /(total|sum|count|avg|amount)/.test(c);
}

function insightExpenseList(rows: Row[]): SqlInsights {
  const totals = { sum: 0, count: 0 };
  const byStatus = new Map<string, { count: number; sum: number }>();
  const byVendor = new Map<string, number>();

  for (const r of rows) {
    const k = lowerKeys(r);
    const amt = num(k.get('total_amount')) ?? 0;
    const status = String(k.get('status') ?? 'unknown');
    const vendor = String(k.get('vendor_name') ?? '—');
    totals.sum += amt;
    totals.count += 1;
    const s = byStatus.get(status) ?? { count: 0, sum: 0 };
    s.count += 1;
    s.sum += amt;
    byStatus.set(status, s);
    byVendor.set(vendor, (byVendor.get(vendor) ?? 0) + amt);
  }

  const avg = totals.count ? totals.sum / totals.count : 0;
  const bullets: SqlInsight[] = [
    { label: 'Total', value: `${thb(totals.sum)} THB`, tone: 'neutral' },
    { label: 'Records', value: String(totals.count), tone: 'neutral' },
    { label: 'Average', value: `${thb(avg)} THB`, tone: 'neutral' },
  ];

  const sortedStatus = Array.from(byStatus.entries()).sort((a, b) => b[1].sum - a[1].sum);
  for (const [s, v] of sortedStatus.slice(0, 4)) {
    bullets.push({
      label: statusLabel(s),
      value: `${thb(v.sum)} THB (${v.count}, ${pct(v.sum, totals.sum)})`,
      tone: s === 'rejected' ? 'negative' : s === 'disbursed' ? 'positive' : 'neutral',
    });
  }

  return {
    headline: `${totals.count} expense records totalling ${thb(totals.sum)} THB`,
    bullets,
  };
}

function insightExpenseItems(rows: Row[]): SqlInsights {
  const byCategory = new Map<string, { name: string; sum: number; count: number }>();
  let total = 0;
  for (const r of rows) {
    const k = lowerKeys(r);
    const amt = num(k.get('amount')) ?? 0;
    const code = String(k.get('mapped_account_code') ?? '—');
    const name = String(k.get('category_name') ?? k.get('category_name_th') ?? code);
    total += amt;
    const c = byCategory.get(code) ?? { name, sum: 0, count: 0 };
    c.sum += amt;
    c.count += 1;
    byCategory.set(code, c);
  }
  const bullets: SqlInsight[] = [
    { label: 'Total items', value: `${thb(total)} THB`, tone: 'neutral' },
    { label: 'Lines', value: String(rows.length), tone: 'neutral' },
  ];
  const sorted = Array.from(byCategory.entries()).sort((a, b) => b[1].sum - a[1].sum);
  for (const [, v] of sorted.slice(0, 4)) {
    bullets.push({
      label: v.name,
      value: `${thb(v.sum)} THB (${v.count})`,
      tone: 'neutral',
    });
  }
  return {
    headline: `${rows.length} line items totalling ${thb(total)} THB across ${sorted.length} categories`,
    bullets,
  };
}

function insightVendorAgg(rows: Row[]): SqlInsights {
  let total = 0;
  for (const r of rows) {
    const k = lowerKeys(r);
    total += num(k.get('total')) ?? num(k.get('total_amount')) ?? num(k.get('sum')) ?? 0;
  }
  const top = rows.slice(0, 3).map((r) => {
    const k = lowerKeys(r);
    const name = String(k.get('vendor_name') ?? '—');
    const amt = num(k.get('total')) ?? num(k.get('total_amount')) ?? num(k.get('sum')) ?? 0;
    return `${name} ${thb(amt)} THB`;
  });
  return {
    headline: `${rows.length} vendors · ${thb(total)} THB total`,
    bullets: [
      { label: 'Total', value: `${thb(total)} THB`, tone: 'neutral' },
      ...top.map((t) => ({ label: 'Top', value: t, tone: 'neutral' as const })),
    ],
  };
}

function insightScalarSum(rows: Row[], columns: string[]): SqlInsights {
  if (rows.length === 0) {
    return { headline: 'No data', bullets: [{ label: columns[0] ?? 'value', value: '—', tone: 'neutral' }] };
  }
  const col = columns[0];
  const c = col.toLowerCase();
  const v = num(rows[0][col]);
  const isCount = /(count|num|qty|quantity|employees|days|records|chunks|pages|contracts|vendors|customers|invoices)/i.test(c);
  const isMoney = /(total|sum|amount|avg|balance|debit|credit|net|cash|vat|salary)/i.test(c);

  if (isCount) {
    return {
      headline: v == null ? `${col}: —` : `${col}: ${v}`,
      bullets: [{ label: col, value: v == null ? '—' : String(v), tone: 'neutral' }],
    };
  }
  if (isMoney) {
    return {
      headline: v == null ? `${col}: —` : `${col}: ${thb(v)} THB`,
      bullets: [{ label: col, value: v == null ? '—' : `${thb(v)} THB`, tone: 'neutral' }],
    };
  }
  return {
    headline: v == null ? `${col}: —` : `${col}: ${v}`,
    bullets: [{ label: col, value: v == null ? '—' : String(v), tone: 'neutral' }],
  };
}

function detectEntityLookup(columns: string[]): boolean {
  const c = new Set(columns.map((x) => x.toLowerCase()));
  return c.has('fullname') || c.has('name') || c.has('display_name');
}

function insightEntityLookup(rows: Row[], columns: string[]): SqlInsights {
  const r = rows[0];
  const k = lowerKeys(r);
  const name = String(k.get('fullname') ?? k.get('name') ?? k.get('display_name') ?? '—');
  const code = String(k.get('employee_code') ?? k.get('code') ?? '');
  const id = String(k.get('id') ?? '');
  const headline = code ? `${name} · ${code}` : name;
  const bullets: SqlInsight[] = [
    { label: 'Name', value: name, tone: 'positive' },
  ];
  if (code) bullets.push({ label: 'Code', value: code, tone: 'neutral' });
  if (id && id !== '—') bullets.push({ label: 'ID', value: id, tone: 'neutral' });
  for (const col of columns) {
    const kc = col.toLowerCase();
    if (['fullname', 'name', 'display_name', 'employee_code', 'code', 'id'].includes(kc)) continue;
    const v = r[col];
    if (v != null && v !== '') bullets.push({ label: col, value: String(v), tone: 'neutral' });
  }
  return { headline, bullets };
}

export function deriveSqlInsights(columns: string[], rows: Row[]): SqlInsights {
  if (!columns.length || !rows.length) {
    return {
      headline: 'No results',
      bullets: [{ label: 'rows', value: '0', tone: 'neutral' }],
    };
  }
  if (detectScalarSum(columns)) return insightScalarSum(rows, columns);
  if (detectEntityLookup(columns) && rows.length === 1) return insightEntityLookup(rows, columns);
  if (detectExpenseItems(columns)) return insightExpenseItems(rows);
  if (detectExpenseList(columns)) return insightExpenseList(rows);
  if (detectVendorAgg(columns)) return insightVendorAgg(rows);
  return {
    headline: `${rows.length} rows returned`,
    bullets: columns.slice(0, 4).map((c) => ({ label: c, value: '—', tone: 'neutral' as const })),
  };
}