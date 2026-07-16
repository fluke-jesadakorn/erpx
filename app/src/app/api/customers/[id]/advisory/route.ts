import { NextResponse } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { getCustomerAdvisory } from '@folio-lib/customer/queries';

export const dynamic = 'force-dynamic';

export async function GET(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const guard = await apiGuard(req, { perm: 'tile:customers:view::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor;
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  const { id } = await ctx.params;
  const customerId = Number(id);
  if (!Number.isFinite(customerId)) {
    return NextResponse.json({ ok: false, error: 'invalid id' }, { status: 400 });
  }
  const lang = (new URL(req.url).searchParams.get('lang') ?? 'en') as 'en' | 'th' | 'de';
  const advisory = await getCustomerAdvisory(customerId, { lang });
  if (!advisory) return NextResponse.json({ ok: true, advisory: null });
  return NextResponse.json({ ok: true, advisory });
}
