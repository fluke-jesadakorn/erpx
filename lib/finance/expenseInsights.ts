import 'server-only';
import { query } from '@/db';

export interface ExpenseInsightRow {
  status: string;
  count: number;
  sum: number;
}

export interface ExpenseInsights {
  total: number;
  count: number;
  avg: number;
  disbursed: number;
  pending: number;
  rejected: number;
  draft: number;
  byStatus: ExpenseInsightRow[];
}

export async function loadExpenseInsights(opts: { submitterId?: number } = {}): Promise<ExpenseInsights> {
  const args: unknown[] = [];
  let where = '';
  if (opts.submitterId) {
    args.push(opts.submitterId);
    where = `WHERE submitter_id = $1`;
  }
  const r = await query<{ status: string; count: string; sum: string }>(
    `SELECT COALESCE(status, 'unknown') AS status,
            COUNT(*)::int AS count,
            COALESCE(SUM(total_amount), 0)::float AS sum
       FROM folio.expenses
       ${where}
       GROUP BY status
       ORDER BY sum DESC`,
    args,
  );
  const rows = r.rows.map((x) => ({
    status: x.status,
    count: Number(x.count),
    sum: Number(x.sum),
  }));
  const total = rows.reduce((s, x) => s + x.sum, 0);
  const count = rows.reduce((s, x) => s + x.count, 0);
  const find = (s: string) => rows.find((x) => x.status === s)?.sum ?? 0;
  const findCount = (s: string) => rows.find((x) => x.status === s)?.count ?? 0;
  const pendingStatuses = new Set([
    'submission',
    'dept_verification',
    'accounting_verification',
    'accounting_review',
    'disbursement_authorization',
    'cfo_authorization',
    'ceo_authorization',
    'awaiting_disbursement',
  ]);
  const pending = rows
    .filter((x) => pendingStatuses.has(x.status))
    .reduce((s, x) => s + x.sum, 0);
  return {
    total,
    count,
    avg: count ? total / count : 0,
    disbursed: find('disbursed'),
    pending,
    rejected: find('rejected'),
    draft: findCount('draft'),
    byStatus: rows,
  };
}