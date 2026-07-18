import { Pool, type PoolClient } from 'pg';

const num = (v: string | undefined, d: number) => (v ? parseInt(v, 10) : d);

export const dbConfig = {
  user: process.env.POSTGRES_USER || 'contract',
  password: process.env.POSTGRES_PASSWORD || 'contractpw',
  host: process.env.POSTGRES_HOST || 'localhost',
  database: process.env.POSTGRES_DB || 'folio_db',
  port: num(process.env.POSTGRES_PORT, 5432),
};

let pool: Pool | null = null;
export function getPool(): Pool {
  if (pool) return pool;
  pool = new Pool({ ...dbConfig, max: 6, idleTimeoutMillis: 10_000 });
  return pool;
}

export async function q<T extends Record<string, unknown> = Record<string, unknown>>(
  text: string,
  params?: unknown[],
): Promise<T[]> {
  const r = await getPool().query<T>(text, params as never);
  return r.rows;
}

export async function q1<T extends Record<string, unknown> = Record<string, unknown>>(
  text: string,
  params?: unknown[],
): Promise<T | null> {
  const rows = await q<T>(text, params);
  return rows[0] ?? null;
}

export async function exec(text: string, params?: unknown[]): Promise<void> {
  await getPool().query(text, params as never);
}

export async function tx(fn: (c: PoolClient) => Promise<void>): Promise<void> {
  const c = await getPool().connect();
  try {
    await c.query('BEGIN');
    await fn(c);
    await c.query('COMMIT');
  } catch (e) {
    await c.query('ROLLBACK').catch(() => {});
    throw e;
  } finally {
    c.release();
  }
}

export async function userIdByCode(code: string): Promise<number> {
  const r = await q1<{ id: number }>(`SELECT id FROM users WHERE employee_code = $1`, [code]);
  if (!r) throw new Error(`User not found: ${code}`);
  return r.id;
}

export async function userIdByCodes(...codes: string[]): Promise<Record<string, number>> {
  const r = await q<{ employee_code: string; id: number }>(
    `SELECT employee_code, id FROM users WHERE employee_code = ANY($1::text[])`,
    [codes],
  );
  const out: Record<string, number> = {};
  for (const row of r) out[row.employee_code] = row.id;
  for (const c of codes) if (!(c in out)) throw new Error(`User not found: ${c}`);
  return out;
}

export async function waybillByExpense(expenseId: number): Promise<string | null> {
  const r = await q1<{ id: string }>(`SELECT id FROM waybills WHERE origin = 'expense' AND origin_id = $1`, [expenseId]);
  return r?.id ?? null;
}

export async function waybillStage(wbId: string): Promise<string | null> {
  const r = await q1<{ current_stage: string }>(`SELECT current_stage FROM waybills WHERE id = $1`, [wbId]);
  return r?.current_stage ?? null;
}

export async function waybillStatus(wbId: string): Promise<string | null> {
  const r = await q1<{ status: string }>(`SELECT status FROM waybills WHERE id = $1`, [wbId]);
  return r?.status ?? null;
}

export async function waybillAmount(wbId: string): Promise<number> {
  const r = await q1<{ total_amount: string | null }>(`SELECT total_amount FROM waybills WHERE id = $1`, [wbId]);
  return r?.total_amount ? parseFloat(r.total_amount) : 0;
}

export async function eventCount(wbId: string): Promise<number> {
  const r = await q1<{ n: string }>(`SELECT count(*)::text AS n FROM waybill_events WHERE waybill_id = $1`, [wbId]);
  return parseInt(r?.n ?? '0', 10);
}

export async function eventKinds(wbId: string): Promise<string[]> {
  const r = await q<{ kind: string }>(`SELECT kind FROM waybill_events WHERE waybill_id = $1 ORDER BY sequence`, [wbId]);
  return r.map(x => x.kind);
}

export async function deleteWaybillByTag(tag: string): Promise<number> {
  const r = await q<{ id: string }>(
    `SELECT w.id FROM waybills w
       LEFT JOIN expenses e ON e.id = w.origin_id AND w.origin = 'expense'
       LEFT JOIN purchase_requisitions pr ON pr.id = w.origin_id AND w.origin = 'pr'
       LEFT JOIN purchase_orders po ON po.id = w.origin_id AND w.origin = 'po'
      WHERE e.vendor_name LIKE $1
         OR pr.dept_group_id LIKE $1
         OR po.po_number LIKE $1
         OR w.id IN (SELECT waybill_id FROM waybill_events WHERE payload::text LIKE $2)`,
    [`${tag}%`, `%${tag}%`],
  );
  const ids = r.map(x => x.id);
  if (ids.length === 0) return 0;
  await exec(`DELETE FROM waybill_events WHERE waybill_id = ANY($1::text[])`, [ids]);
  await exec(`DELETE FROM waybill_attachments WHERE waybill_id = ANY($1::text[])`, [ids]);
  await exec(`DELETE FROM waybills WHERE id = ANY($1::text[])`, [ids]);
  return ids.length;
}

export async function deleteExpenseByTag(tag: string): Promise<number> {
  const r = await q<{ id: number }>(`SELECT id FROM expenses WHERE vendor_name LIKE $1`, [`${tag}%`]);
  const ids = r.map(x => x.id);
  if (ids.length === 0) return 0;
  await exec(`DELETE FROM slips WHERE expense_id = ANY($1::int[])`, [ids]);
  await exec(`DELETE FROM expense_items WHERE expense_id = ANY($1::int[])`, [ids]);
  await exec(`DELETE FROM expenses WHERE id = ANY($1::int[])`, [ids]);
  return ids.length;
}

export async function deleteSlipsByUploader(userId: number, tag: string): Promise<number> {
  const r = await q<{ id: number }>(`SELECT id FROM slips WHERE uploaded_by = $1 AND (file_path LIKE $2 OR ocr_raw_json::text LIKE $2)`, [userId, `%${tag}%`]);
  const ids = r.map(x => x.id);
  if (ids.length === 0) return 0;
  await exec(`DELETE FROM slips WHERE id = ANY($1::int[])`, [ids]);
  return ids.length;
}

export async function cleanupTestTag(tag: string): Promise<{ waybills: number; expenses: number; slips: number }> {
  const w = await deleteWaybillByTag(tag);
  const e = await deleteExpenseByTag(tag);
  await exec(`DELETE FROM slips WHERE ocr_raw_json::text LIKE $1 OR file_path LIKE $1`, [`%${tag}%`]);
  const s = await q1<{ n: string }>(`SELECT count(*)::text AS n FROM slips WHERE ocr_raw_json::text LIKE $1 OR file_path LIKE $1`, [`%${tag}%`]);
  return { waybills: w, expenses: e, slips: parseInt(s?.n ?? '0', 10) };
}

export async function vendorTag(): Promise<string> {
  return `E2E_${Date.now().toString(36)}_${Math.floor(Math.random() * 1e6).toString(36)}`;
}
