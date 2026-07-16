import { NextResponse } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { searchCustomers } from '@folio-lib/customer/queries';
import { searchProductsSemantic } from '@folio-lib/customer/embedProducts';

export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const guard = await apiGuard(req, { perm: 'tile:customers:view::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor;
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  const url = new URL(req.url);
  const q = (url.searchParams.get('q') ?? '').trim();
  const rawLimit = parseInt(url.searchParams.get('limit') ?? '10', 10);
  const limit = Math.min(Math.max(isFinite(rawLimit) ? rawLimit : 10, 1), 50);

  try {
    const [customerRows, productRows] = await Promise.all([
      searchCustomers(q, limit),
      searchProductsSemantic(q, limit).catch(() => []),
    ]);
    const customers = customerRows.map((r) => ({
      id: r.id,
      code: r.code,
      name: r.name,
      name_th: r.name_th,
      credit_limit_thb: r.credit_limit_thb,
      payment_terms: r.payment_terms,
      blacklist: r.blacklist,
      is_active: r.is_active,
    }));
    return NextResponse.json({
      ok: true,
      customers,
      results: customers,
      products: productRows,
    });
  } catch (e: any) {
    return NextResponse.json({ ok: false, error: e?.message ?? 'unknown' }, { status: 500 });
  }
}
