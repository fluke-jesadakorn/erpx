import 'server-only';
import { query } from '@folio-lib/db';
import { aiInvoke } from '@folio-lib/ai/router';
import { customerCreditPrompt, renderLocaleAwarePrompt } from '@folio-lib/ai/systemPrompts';

export interface CustomerRow {
  id: number;
  code: string;
  name: string;
  name_th: string | null;
  tax_id: string | null;
  billing_address: string | null;
  shipping_address: string | null;
  contact_name: string | null;
  contact_email: string | null;
  contact_phone: string | null;
  credit_limit_thb: number;
  payment_terms: string;
  blacklist: boolean;
  is_active: boolean;
}

export interface CustomerArHistory {
  customer_id: number;
  customer_code: string;
  customer_name: string;
  credit_limit: number;
  total_invoiced: number;
  outstanding_ar: number;
  total_paid: number;
  so_count: number;
}

export interface CustomerContactRow {
  id: number;
  customer_id: number;
  fullname: string;
  role: string | null;
  email: string | null;
  phone: string | null;
  notes: string | null;
}

const CUSTOMER_COLS = `
  id, code, name, name_th, tax_id, billing_address, shipping_address,
  contact_name, contact_email, contact_phone,
  credit_limit_thb::float8 AS credit_limit_thb,
  payment_terms, blacklist, is_active
`;

function rowToCustomer(r: any): CustomerRow {
  return {
    id: Number(r.id),
    code: r.code,
    name: r.name,
    name_th: r.name_th ?? null,
    tax_id: r.tax_id ?? null,
    billing_address: r.billing_address ?? null,
    shipping_address: r.shipping_address ?? null,
    contact_name: r.contact_name ?? null,
    contact_email: r.contact_email ?? null,
    contact_phone: r.contact_phone ?? null,
    credit_limit_thb: Number(r.credit_limit_thb ?? 0),
    payment_terms: r.payment_terms ?? 'Net 30',
    blacklist: !!r.blacklist,
    is_active: !!r.is_active,
  };
}

export async function listCustomers(opts?: {
  search?: string;
  activeOnly?: boolean;
  limit?: number;
  offset?: number;
}): Promise<CustomerRow[]> {
  try {
    const where: string[] = [];
    const params: unknown[] = [];
    if (opts?.activeOnly) where.push('is_active = TRUE');
    if (opts?.search && opts.search.trim()) {
      params.push(`%${opts.search.trim()}%`);
      const p = params.length;
      where.push(`(name ILIKE $${p} OR name_th ILIKE $${p} OR code ILIKE $${p} OR tax_id ILIKE $${p})`);
    }
    params.push(opts?.limit ?? 200);
    const limitIdx = params.length;
    params.push(opts?.offset ?? 0);
    const offsetIdx = params.length;
    const whereSql = where.length ? `WHERE ${where.join(' AND ')}` : '';
    const r = await query<any>(
      `SELECT ${CUSTOMER_COLS}
         FROM customers
         ${whereSql}
      ORDER BY name ASC
         LIMIT $${limitIdx} OFFSET $${offsetIdx}`,
      params,
    );
    return r.rows.map(rowToCustomer);
  } catch (error: any) {
    console.error('listCustomers failed:', error);
    return [];
  }
}

export async function getCustomer(id: number): Promise<CustomerRow | null> {
  try {
    const r = await query<any>(
      `SELECT ${CUSTOMER_COLS} FROM customers WHERE id = $1`,
      [id],
    );
    if (r.rows.length === 0) return null;
    return rowToCustomer(r.rows[0]);
  } catch (error: any) {
    console.error('getCustomer failed:', error);
    return null;
  }
}

export async function getCustomerArHistory(id: number): Promise<CustomerArHistory | null> {
  try {
    const r = await query<any>(
      `SELECT c.id AS customer_id,
              c.code AS customer_code,
              c.name AS customer_name,
              c.credit_limit_thb::float8 AS credit_limit,
              COALESCE(SUM(CASE WHEN so.status = 'so_invoiced' THEN so.total_amount ELSE 0 END), 0)::float8 AS total_invoiced,
              COALESCE(SUM(CASE WHEN so.status = 'so_invoiced' THEN so.total_amount ELSE 0 END), 0)::float8 AS outstanding_ar,
              COALESCE(SUM(CASE WHEN so.status = 'so_paid' THEN so.total_amount ELSE 0 END), 0)::float8 AS total_paid,
              COUNT(so.id)::int AS so_count
         FROM customers c
         LEFT JOIN sales_orders so ON so.customer_id = c.id
        WHERE c.id = $1
     GROUP BY c.id, c.code, c.name, c.credit_limit_thb`,
      [id],
    );
    const row = r.rows[0];
    if (!row) return null;
    return {
      customer_id: Number(row.customer_id),
      customer_code: row.customer_code,
      customer_name: row.customer_name,
      credit_limit: Number(row.credit_limit ?? 0),
      total_invoiced: Number(row.total_invoiced ?? 0),
      outstanding_ar: Number(row.outstanding_ar ?? 0),
      total_paid: Number(row.total_paid ?? 0),
      so_count: Number(row.so_count ?? 0),
    };
  } catch (error: any) {
    console.error('getCustomerArHistory failed:', error);
    return null;
  }
}

export async function listCustomerContacts(customerId: number): Promise<CustomerContactRow[]> {
  try {
    const r = await query<any>(
      `SELECT id, customer_id, fullname, role, email, phone, notes
         FROM customer_contacts
        WHERE customer_id = $1
     ORDER BY id ASC`,
      [customerId],
    );
    return r.rows.map((row: any) => ({
      id: Number(row.id),
      customer_id: Number(row.customer_id),
      fullname: row.fullname,
      role: row.role ?? null,
      email: row.email ?? null,
      phone: row.phone ?? null,
      notes: row.notes ?? null,
    }));
  } catch (error: any) {
    console.error('listCustomerContacts failed:', error);
    return [];
  }
}

export async function searchCustomersSemantic(
  q: string,
  limit = 10
): Promise<CustomerRow[]> {
  const text = q.trim();
  if (!text) return [];
  const r = await aiInvoke('acct:coa-search', 'embed', { text, modelOverride: 'bge-m3' });
  if (!r.ok || !r.embedding || r.embedding.length !== 1024) return [];
  const vec = `[${r.embedding.join(',')}]`;
  const out = await query<any>(
    `SELECT id, code, name, name_th, tax_id, billing_address, shipping_address,
            contact_name, contact_email, contact_phone,
            credit_limit_thb::float8 AS credit_limit_thb,
            payment_terms, blacklist, is_active,
            1 - (embedding <=> $1::vector) AS score
       FROM folio.customers
      WHERE embedding IS NOT NULL
        AND is_active = TRUE
   ORDER BY embedding <=> $1::vector
      LIMIT $2`,
    [vec, Math.min(Math.max(limit, 1), 30)]
  );
  return out.rows.map(row => ({
    id: Number(row.id),
    code: row.code,
    name: row.name,
    name_th: row.name_th ?? null,
    tax_id: row.tax_id ?? null,
    billing_address: row.billing_address ?? null,
    shipping_address: row.shipping_address ?? null,
    contact_name: row.contact_name ?? null,
    contact_email: row.contact_email ?? null,
    contact_phone: row.contact_phone ?? null,
    credit_limit_thb: Number(row.credit_limit_thb ?? 0),
    payment_terms: row.payment_terms ?? 'Net 30',
    blacklist: !!row.blacklist,
    is_active: !!row.is_active,
  }));
}

export async function searchCustomers(q: string, limit = 20): Promise<CustomerRow[]> {
  const text = q.trim();
  if (!text) return [];
  const ilike = await listCustomers({ search: text, activeOnly: true, limit });
  if (ilike.length >= 3) return ilike;
  const sem = await searchCustomersSemantic(text, limit).catch(() => [] as CustomerRow[]);
  const seen = new Set<number>();
  const merged: CustomerRow[] = [];
  for (const r of [...ilike, ...sem]) {
    if (seen.has(r.id)) continue;
    seen.add(r.id);
    merged.push(r);
    if (merged.length >= limit) break;
  }
  return merged;
}

export async function createCustomer(input: Omit<CustomerRow, 'id'>): Promise<CustomerRow> {
  try {
    const r = await query<any>(
      `INSERT INTO customers
         (code, name, name_th, tax_id, billing_address, shipping_address,
          contact_name, contact_email, contact_phone,
          credit_limit_thb, payment_terms, blacklist, is_active)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
       RETURNING ${CUSTOMER_COLS}`,
      [
        input.code,
        input.name,
        input.name_th ?? null,
        input.tax_id ?? null,
        input.billing_address ?? null,
        input.shipping_address ?? null,
        input.contact_name ?? null,
        input.contact_email ?? null,
        input.contact_phone ?? null,
        input.credit_limit_thb ?? 0,
        input.payment_terms ?? 'Net 30',
        input.blacklist ?? false,
        input.is_active ?? true,
      ],
    );
    return rowToCustomer(r.rows[0]);
  } catch (error: any) {
    console.error('createCustomer failed:', error);
    throw error;
  }
}

export async function updateCustomer(id: number, patch: Partial<CustomerRow>): Promise<CustomerRow | null> {
  try {
    const fields: string[] = [];
    const params: unknown[] = [];
    const set = (col: string, val: unknown) => {
      params.push(val);
      fields.push(`${col} = $${params.length}`);
    };
    if (patch.code !== undefined) set('code', patch.code);
    if (patch.name !== undefined) set('name', patch.name);
    if (patch.name_th !== undefined) set('name_th', patch.name_th);
    if (patch.tax_id !== undefined) set('tax_id', patch.tax_id);
    if (patch.billing_address !== undefined) set('billing_address', patch.billing_address);
    if (patch.shipping_address !== undefined) set('shipping_address', patch.shipping_address);
    if (patch.contact_name !== undefined) set('contact_name', patch.contact_name);
    if (patch.contact_email !== undefined) set('contact_email', patch.contact_email);
    if (patch.contact_phone !== undefined) set('contact_phone', patch.contact_phone);
    if (patch.credit_limit_thb !== undefined) set('credit_limit_thb', patch.credit_limit_thb);
    if (patch.payment_terms !== undefined) set('payment_terms', patch.payment_terms);
    if (patch.blacklist !== undefined) set('blacklist', patch.blacklist);
    if (patch.is_active !== undefined) set('is_active', patch.is_active);
    if (fields.length === 0) return getCustomer(id);
    params.push(id);
    const r = await query<any>(
      `UPDATE customers SET ${fields.join(', ')} WHERE id = $${params.length}
       RETURNING ${CUSTOMER_COLS}`,
      params,
    );
    if (r.rows.length === 0) return null;
    return rowToCustomer(r.rows[0]);
  } catch (error: any) {
    console.error('updateCustomer failed:', error);
    return null;
  }
}

export async function blacklistCustomer(id: number, value: boolean): Promise<void> {
  try {
    await query(`UPDATE customers SET blacklist = $1 WHERE id = $2`, [value, id]);
  } catch (error: any) {
    console.error('blacklistCustomer failed:', error);
  }
}

export interface CustomerAdvisory {
  customerId: number;
  customerCode: string;
  customerName: string;
  advisory: string;
  severity: 'ok' | 'watch' | 'critical';
  computedAt: string;
}

export async function getCustomerAdvisory(
  customerId: number,
  opts?: { lang?: 'en' | 'th' | 'de' }
): Promise<CustomerAdvisory | null> {
  const lang = opts?.lang ?? 'en';
  const hist = await getCustomerArHistory(customerId);
  if (!hist) return null;

  const r = await aiInvoke('customer:advisory', 'chat', {
    systemPrompt: renderLocaleAwarePrompt(customerCreditPrompt, lang),
    text: JSON.stringify({
      customerCode: hist.customer_code,
      customerName: hist.customer_name,
      creditLimit: hist.credit_limit,
      outstandingAr: hist.outstanding_ar,
      totalPaid: hist.total_paid,
      soCount: hist.so_count,
    }),
    temperature: 0.1,
    maxTokens: 300,
  });
  if (!r.ok || !r.text) return null;

  const advisory = r.text.trim();
  const lower = advisory.toLowerCase();
  const severity: CustomerAdvisory['severity'] =
    hist.outstanding_ar > hist.credit_limit ? 'critical'
      : lower.includes('limit') || lower.includes('concern') || lower.includes('เกิน') ? 'watch'
        : 'ok';

  return {
    customerId: hist.customer_id,
    customerCode: hist.customer_code,
    customerName: hist.customer_name,
    advisory,
    severity,
    computedAt: new Date().toISOString(),
  };
}