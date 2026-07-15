// lib/waybill/number.ts — generate and parse Waybill IDs of the form
// WB-YYYY-NNNNNN. The dense sequence is owned by the DB function
// next_waybill_number(); we only call it.

import 'server-only';
import { query } from '../db';

const WB_REGEX = /^WB-(\d{4})-(\d{6})$/;

export interface WaybillNumberParts {
  fiscalYear: number;
  sequence: number;
  id: string;
}

export function parseWaybillId(raw: string): WaybillNumberParts | null {
  const m = WB_REGEX.exec(raw);
  if (!m) return null;
  return {
    fiscalYear: Number(m[1]),
    sequence: Number(m[2]),
    id: raw,
  };
}

export async function generateWaybillId(fiscalYear: number): Promise<string> {
  const r = await query<{ next_waybill_number: string }>(
    `SELECT next_waybill_number($1::smallint) AS next_waybill_number`,
    [fiscalYear],
  );
  return r.rows[0]?.next_waybill_number ?? `WB-${fiscalYear}-000001`;
}

export function currentFiscalYear(now: Date = new Date()): number {
  return now.getFullYear();
}
