import 'server-only';
import { query, withTransaction } from '../db';

export interface JobRow {
  id: string;
  contract_id: string;
  raw_text: string | null;
  status: 'pending' | 'running' | 'done' | 'failed';
  attempts: number;
  last_error: string | null;
  enqueued_at: Date;
  started_at: Date | null;
  finished_at: Date | null;
}

export async function enqueueIndexing(args: { contractId: string; rawText?: string }): Promise<JobRow> {
  const r = await query<JobRow>(
    `INSERT INTO law.job_queue (contract_id, raw_text, status)
     VALUES ($1, $2, 'pending')
     RETURNING id, contract_id, raw_text, status, attempts, last_error, enqueued_at, started_at, finished_at`,
    [args.contractId, args.rawText ?? null]
  );
  return r.rows[0];
}

export async function claimNextJob(): Promise<JobRow | null> {
  return withTransaction(async (q) => {
    const sel = await q<JobRow>(
      `SELECT id, contract_id, raw_text, status, attempts, last_error, enqueued_at, started_at, finished_at
         FROM law.job_queue
        WHERE status = 'pending'
        ORDER BY enqueued_at ASC
        FOR UPDATE SKIP LOCKED
        LIMIT 1`
    );
    if (sel.rows.length === 0) return null;
    const row = sel.rows[0];
    await q(
      `UPDATE law.job_queue
          SET status = 'running',
              attempts = attempts + 1,
              started_at = now()
        WHERE id = $1`,
      [row.id]
    );
    return { ...row, status: 'running' };
  });
}

export async function markDone(jobId: string): Promise<void> {
  await query(
    `UPDATE law.job_queue
        SET status = 'done', finished_at = now()
      WHERE id = $1`,
    [jobId]
  );
}

export async function markFailed(jobId: string, error: string): Promise<void> {
  await query(
    `UPDATE law.job_queue
        SET status = 'failed',
            last_error = $2,
            finished_at = now()
      WHERE id = $1`,
    [jobId, error.slice(0, 2000)]
  );
}