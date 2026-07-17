import { NextRequest, NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { getCashflowStatement } from '@/finance/cashflow';

export const dynamic = 'force-dynamic';
export const runtime = 'nodejs';

export async function GET(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'finance:cashflow:read::allow' });
  if (guard.response) return guard.response;
  const { searchParams } = new URL(req.url);
  const dateFrom = searchParams.get('date_from') ?? '';
  const dateTo = searchParams.get('date_to') ?? '';
  const lang = (searchParams.get('lang') ?? 'en') as 'en' | 'th' | 'de';
  if (!dateFrom || !dateTo) {
    return NextResponse.json({ ok: false, error: 'date_from and date_to required (YYYY-MM-DD)' }, { status: 400 });
  }
  const result = await getCashflowStatement({ dateFrom, dateTo, lang });
  return NextResponse.json({ ok: true, statement: result });
}

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'finance:cashflow:read::allow' });
  if (guard.response) return guard.response;
  const body = await req.json().catch(() => null);
  const dateFrom = String(body?.date_from ?? '').trim();
  const dateTo = String(body?.date_to ?? '').trim();
  const lang = (body?.lang ?? 'en') as 'en' | 'th' | 'de';
  if (!dateFrom || !dateTo) {
    return NextResponse.json({ ok: false, error: 'date_from and date_to required (YYYY-MM-DD)' }, { status: 400 });
  }
  const result = await getCashflowStatement({ dateFrom, dateTo, lang });
  return NextResponse.json({ ok: true, statement: result });
}