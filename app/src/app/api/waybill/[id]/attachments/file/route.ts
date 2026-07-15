import { NextResponse } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { loadWaybill } from '@folio-lib/waybill/queries';
import { PERM } from '@folio-lib/perm';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request, ctx: { params: Promise<{ id: string }> }): Promise<Response> {
  const guard = await apiGuard(req, { perm: PERM.finance.expense.view_own });
  if (guard.response) {
    const alt = await apiGuard(req, { perm: PERM.finance.expense.view_all });
    if (alt.response) return alt.response;
  }
  const { id } = await ctx.params;
  const wb = await loadWaybill(id);
  if (!wb) return NextResponse.json({ error: 'not found' }, { status: 404 });

  const url = new URL(req.url);
  const key = url.searchParams.get('key');
  if (!key) return NextResponse.json({ error: 'key required' }, { status: 400 });
  if (!key.startsWith(`waybill-attachments/${id}/`)) {
    return NextResponse.json({ error: 'key does not belong to this waybill' }, { status: 403 });
  }
  return NextResponse.redirect(new URL(`/api/slips/file?key=${encodeURIComponent(key)}`, req.url), 302);
}
