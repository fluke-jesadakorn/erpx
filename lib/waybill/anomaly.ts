import 'server-only';
import { query } from '../db';

export type AnomalyCode =
  | 'vendor_high_frequency'
  | 'amount_duplicate_30d'
  | 'amount_above_submitter_avg'
  | 'time_after_hours';

export interface AnomalyFlag {
  code: AnomalyCode;
  severity: 'warning' | 'error';
  message: string;
  weight: number;
}

export interface AnomalyReport {
  submitterId: number;
  vendorName: string | null;
  totalAmount: number;
  flags: AnomalyFlag[];
  score: number;
  generatedAt: string;
}

const WEIGHTS: Record<AnomalyCode, number> = {
  vendor_high_frequency: 30,
  amount_duplicate_30d: 35,
  amount_above_submitter_avg: 20,
  time_after_hours: 15,
};

function round2(n: number): number {
  return Math.round(n * 100) / 100;
}

export async function detectAnomalies(args: {
  submitterId: number;
  vendorName: string | null;
  totalAmount: number;
  transactionDate?: string | null;
}): Promise<AnomalyReport> {
  const flags: AnomalyFlag[] = [];
  const vendor = args.vendorName?.trim() || null;

  if (vendor) {
    const vendorFreq = await query<{ n: string }>(
      `SELECT COUNT(*)::text AS n
         FROM expenses
        WHERE submitter_id = $1
          AND vendor_name = $2
          AND created_at > now() - INTERVAL '90 days'`,
      [args.submitterId, vendor]
    );
    const n = Number(vendorFreq.rows[0]?.n ?? 0);
    if (n >= 5) {
      flags.push({
        code: 'vendor_high_frequency',
        severity: n >= 10 ? 'error' : 'warning',
        message: `Vendor "${vendor}" reimbursed ${n}× in the last 90 days`,
        weight: WEIGHTS.vendor_high_frequency,
      });
    }
  }

  if (Number.isFinite(args.totalAmount) && args.totalAmount > 0) {
    const dupRes = await query<{ n: string }>(
      `SELECT COUNT(*)::text AS n
         FROM expenses
        WHERE submitter_id = $1
          AND total_amount = $2
          AND created_at > now() - INTERVAL '30 days'`,
      [args.submitterId, args.totalAmount]
    );
    const dupCount = Number(dupRes.rows[0]?.n ?? 0);
    if (dupCount >= 1) {
      flags.push({
        code: 'amount_duplicate_30d',
        severity: dupCount >= 2 ? 'error' : 'warning',
        message: `Identical amount ${round2(args.totalAmount)} THB already submitted ${dupCount}× in last 30 days`,
        weight: WEIGHTS.amount_duplicate_30d,
      });
    }

    const avgRes = await query<{ avg: string | null }>(
      `SELECT AVG(total_amount)::text AS avg
         FROM expenses
        WHERE submitter_id = $1
          AND created_at > now() - INTERVAL '180 days'`,
      [args.submitterId]
    );
    const avg = avgRes.rows[0]?.avg ? parseFloat(avgRes.rows[0].avg) : 0;
    if (avg > 0 && args.totalAmount > avg * 5) {
      flags.push({
        code: 'amount_above_submitter_avg',
        severity: args.totalAmount > avg * 10 ? 'error' : 'warning',
        message: `Amount ${round2(args.totalAmount)} THB is ${round2(args.totalAmount / avg)}× the submitter's 180-day average`,
        weight: WEIGHTS.amount_above_submitter_avg,
      });
    }
  }

  if (args.transactionDate) {
    const d = new Date(args.transactionDate);
    if (!Number.isNaN(d.getTime())) {
      const hour = d.getUTCHours();
      if (hour >= 22 || hour < 4) {
        flags.push({
          code: 'time_after_hours',
          severity: 'warning',
          message: `Transaction date ${args.transactionDate} falls between 22:00 and 04:00 UTC`,
          weight: WEIGHTS.time_after_hours,
        });
      }
    }
  }

  const score = Math.min(100, flags.reduce((s, f) => s + f.weight, 0));

  return {
    submitterId: args.submitterId,
    vendorName: vendor,
    totalAmount: args.totalAmount,
    flags,
    score,
    generatedAt: new Date().toISOString(),
  };
}

export async function flagWaybill(waybillId: string, report: AnomalyReport): Promise<void> {
  const summary = report.flags.length === 0
    ? null
    : JSON.stringify({
        score: report.score,
        flags: report.flags.map(f => ({ code: f.code, severity: f.severity, message: f.message })),
        generatedAt: report.generatedAt,
      });

  await query(
    `UPDATE folio.waybills
        SET flagged_reason = $2,
            updated_at = now()
      WHERE id = $1`,
    [waybillId, summary]
  );
}