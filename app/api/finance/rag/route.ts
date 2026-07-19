import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { askFinance, searchVendors } from '@/finance/rag';

export const dynamic = 'force-dynamic';

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: 'finance:expense:view_own::allow' });
  if (guard.response) return guard.response;

  let body: any = {};
  try { body = await req.json(); } catch {}
  const q = String(body?.question ?? body?.query ?? '').trim();
  if (!q) return NextResponse.json({ ok: false, error: 'missing question' }, { status: 400 });

  const lang = (body.lang ?? 'en') as 'en' | 'th' | 'de';
  const mode = (body.mode ?? 'answer') as 'answer' | 'search';

  if (mode === 'search') {
    const hits = await searchVendors({
      query: q,
      k: Number(body.k) || 10,
      dateFrom: body.dateFrom ?? null,
      dateTo: body.dateTo ?? null,
      amountMin: Number.isFinite(body.amountMin) ? body.amountMin : null,
      amountMax: Number.isFinite(body.amountMax) ? body.amountMax : null,
      actorId: guard.actor?.id,
    });
    return NextResponse.json({ ok: true, hits });
  }

  const r = await askFinance(q, lang, guard.actor?.id);
  if (!r) return NextResponse.json({ ok: true, answer: null, hits: [] });
  return NextResponse.json({ ok: true, ...r });
}
