import { NextResponse } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { attachArReceiptAction } from '@/app/(app)/(protected)/sales/[id]/_actions';

export const dynamic = 'force-dynamic';

export async function POST(req: Request, { params }: { params: Promise<{ id: string }> }) {
  const guard = await apiGuard(req, { perm: 'finance:sales:settle::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor;
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  const { id: waybillId } = await params;
  const body = await req.json().catch(() => ({}));
  const slipId = body.slipId ?? body.slip_id;
  if (!slipId) return NextResponse.json({ ok: false, error: 'slipId required' }, { status: 400 });

  const fd = new FormData();
  fd.set('waybillId', waybillId);
  fd.set('slipId', String(slipId));

  try {
    await attachArReceiptAction(fd);
    return NextResponse.json({ ok: true });
  } catch (e: any) {
    if (typeof e?.digest === 'string' && e.digest.startsWith('NEXT_REDIRECT')) {
      return NextResponse.json({ ok: true, redirected: true });
    }
    console.error('attachArReceipt error', e);
    return NextResponse.json({ ok: false, error: e?.message ?? 'unknown' }, { status: 500 });
  }
}
