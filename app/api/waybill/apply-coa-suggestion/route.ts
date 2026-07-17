import { NextResponse } from 'next/server';
import { loadActivePermSession } from '@/perm/server';
import { applyCoaSuggestionAction } from '@/app/actions/coa';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface Body {
  itemId?: number;
  code?: string;
  normalSide?: 'debit' | 'credit';
  expenseId?: number;
}

export async function POST(req: Request) {
  const session = await loadActivePermSession(req);
  if (!session) {
    return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });
  }

  const body = (await req.json().catch(() => ({}))) as Body;
  const itemId = Number(body.itemId);
  const expenseId = Number(body.expenseId);
  const code = typeof body.code === 'string' ? body.code : '';
  const normalSide: 'debit' | 'credit' = body.normalSide === 'credit' ? 'credit' : 'debit';

  if (!Number.isFinite(itemId) || itemId <= 0
      || !Number.isFinite(expenseId) || expenseId <= 0
      || !code) {
    return NextResponse.json({ ok: false, error: 'invalid input' }, { status: 400 });
  }

  const result = await applyCoaSuggestionAction({
    itemId,
    code,
    normalSide,
    expenseId,
  });

  if (result.ok) {
    return NextResponse.json({ ok: true });
  }
  const status = result.error === 'unauthorized' ? 401
    : result.error === 'cannot act at this stage' ? 403
    : 400;
  return NextResponse.json({ ok: false, error: result.error }, { status });
}
