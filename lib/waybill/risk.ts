import 'server-only';
import { query } from '../db';

export async function computeRiskScore(waybillId: string): Promise<number> {
  const r = await query<{ score: number }>(
    `SELECT folio.waybill_risk_score($1) AS score`,
    [waybillId]
  );
  return Number(r.rows[0]?.score ?? 0);
}

export async function computeRiskScores(waybillIds: string[]): Promise<Map<string, number>> {
  if (waybillIds.length === 0) return new Map();
  const r = await query<{ id: string; score: number }>(
    `SELECT id, folio.waybill_risk_score(id) AS score
       FROM folio.waybills
      WHERE id = ANY($1::text[])`,
    [waybillIds]
  );
  const m = new Map<string, number>();
  for (const row of r.rows) m.set(row.id, Number(row.score));
  return m;
}