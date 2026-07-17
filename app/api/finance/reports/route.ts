import { NextRequest, NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { resolveReport } from '@/finance/reports';

export const dynamic = 'force-dynamic';
export const runtime = 'nodejs';

const SUPPORTED = new Set(['cash_flow', 'trial_balance', 'income_statement', 'balance_sheet', 'period_summary']);

export async function GET(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'finance:cashflow:read::allow' });
  if (guard.response) return guard.response;
  const { searchParams } = new URL(req.url);
  const intent = String(searchParams.get('intent') ?? '').trim();
  const dateFrom = String(searchParams.get('date_from') ?? '').trim();
  const dateTo = String(searchParams.get('date_to') ?? '').trim();
  const lang = (searchParams.get('lang') ?? 'en') as 'en' | 'th' | 'de';
  return runResolve(intent, dateFrom, dateTo, lang);
}

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'finance:cashflow:read::allow' });
  if (guard.response) return guard.response;
  const body = await req.json().catch(() => null);
  const intent = String(body?.intent ?? '').trim();
  const dateFrom = String(body?.date_from ?? '').trim();
  const dateTo = String(body?.date_to ?? '').trim();
  const lang = (body?.lang ?? 'en') as 'en' | 'th' | 'de';
  return runResolve(intent, dateFrom, dateTo, lang);
}

function runResolve(intent: string, dateFrom: string, dateTo: string, lang: 'en' | 'th' | 'de') {
  if (!SUPPORTED.has(intent)) {
    return NextResponse.json({ ok: false, error: `unsupported intent: ${intent}` }, { status: 400 });
  }
  if (!dateFrom || !dateTo) {
    return NextResponse.json({ ok: false, error: 'date_from and date_to required (YYYY-MM-DD)' }, { status: 400 });
  }
  return resolveReport({
    intent: intent as 'cash_flow' | 'trial_balance' | 'income_statement' | 'balance_sheet' | 'period_summary',
    dateFrom,
    dateTo,
    lang,
  }).then((r) => NextResponse.json({ ok: true, report: r }));
}