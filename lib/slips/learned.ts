import 'server-only';
import { query } from '../db';
import { normalizeVendorName } from './normalize';

export interface LearnedMapping {
  vendorNameNorm: string;
  accountCode: string;
  hits: number;
}

export async function getLearnedMapping(vendorName: string): Promise<LearnedMapping | null> {
  const norm = normalizeVendorName(vendorName);
  if (!norm) return null;
  const r = await query<{ account_code: string; hits: string | number }>(
    `SELECT account_code, hits::text FROM learned_mappings
      WHERE vendor_name_norm = $1
   ORDER BY hits DESC, last_used_at DESC
      LIMIT 1`,
    [norm]
  );
  if (r.rows.length === 0) return null;
  return {
    vendorNameNorm: norm,
    accountCode: r.rows[0].account_code,
    hits: Number(r.rows[0].hits),
  };
}

export async function recordLearning(vendorName: string, accountCode: string): Promise<void> {
  const norm = normalizeVendorName(vendorName);
  if (!norm || !accountCode) return;
  await query(
    `INSERT INTO learned_mappings (vendor_name_norm, account_code, hits, last_used_at)
     VALUES ($1, $2, 1, now())
     ON CONFLICT (vendor_name_norm, account_code)
     DO UPDATE SET hits = learned_mappings.hits + 1, last_used_at = now()`,
    [norm, accountCode]
  );
}