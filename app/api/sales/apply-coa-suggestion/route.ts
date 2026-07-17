import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { applySoCoaAction } from '@folio-lib/sales/coa';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface Body {
  soId?: number;
  itemId?: number;
  code?: string;
  waybillId?: string;
}

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: 'stage:so_paid:act::allow' });
  if (guard.response) return guard.response;

  const body = (await req.json().catch(() => ({}))) as Body;
  const soId = Number(body.soId);
  const itemId = Number(body.itemId);
  const code = typeof body.code === 'string' ? body.code : '';
  const waybillId = typeof body.waybillId === 'string' ? body.waybillId : '';

  if (!Number.isFinite(soId) || soId <= 0
      || !Number.isFinite(itemId) || itemId <= 0
      || !code
      || !waybillId) {
    return NextResponse.json({ ok: false, error: 'invalid input' }, { status: 400 });
  }

  try {
    await applySoCoaAction({
      soId,
      itemId,
      code,
      waybillId,
      actorId: guard.actor.id,
      actorPerms: new Set(guard.actor.permissions),
    });
    return NextResponse.json({ ok: true });
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : 'apply failed';
    return NextResponse.json({ ok: false, error: msg }, { status: 400 });
  }
}