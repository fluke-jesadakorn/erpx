import { NextRequest, NextResponse } from 'next/server';
import { claimNextJob, markDone, markFailed } from '@/law/queue';
import { runIndexingJob } from '@/law/chunks';
import { isTrustedWorkerRequest } from '@/server/internalAuth';

export const dynamic = 'force-dynamic';
export const runtime = 'nodejs';

export async function POST(req: NextRequest) {
  if (!isTrustedWorkerRequest(req)) {
    return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });
  }
  const job = await claimNextJob();
  if (!job) return NextResponse.json({ ok: true, processed: 0 });

  try {
    const count = await runIndexingJob(job.id, job.contract_id, job.raw_text);
    await markDone(job.id);
    return NextResponse.json({ ok: true, processed: 1, jobId: job.id, contractId: job.contract_id, chunks: count });
  } catch (e: any) {
    await markFailed(job.id, e?.message ?? String(e));
    return NextResponse.json({ ok: false, processed: 1, jobId: job.id, error: e?.message ?? String(e) }, { status: 500 });
  }
}
