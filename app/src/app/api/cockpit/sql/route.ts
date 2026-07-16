import { NextRequest, NextResponse } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { askSql } from '@folio-lib/ai/sql';

export const dynamic = 'force-dynamic';

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'tile:cockpit:view::allow' });
  if (guard.response) return guard.response;

  const body = await req.json().catch(() => null);
  const question = String(body?.question ?? body?.query ?? '').trim();
  if (!question) return NextResponse.json({ ok: false, error: 'missing question' }, { status: 400 });

  const lang = (body?.lang ?? 'en') as 'en' | 'th' | 'de';
  const result = await askSql({ question, lang });
  if (!result) return NextResponse.json({ ok: false, error: 'AI failed' }, { status: 502 });
  return NextResponse.json({ ok: true, result });
}