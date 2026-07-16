import { NextResponse } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { askSql } from '@folio-lib/ai/sql';

export const dynamic = 'force-dynamic';

export async function POST(req: Request) {
  const g = await apiGuard(req, { perm: 'tile:expense:view::allow' });
  if (g.response) return g.response;
  const body = await req.json().catch(() => ({}));
  const question = String(body.question ?? '').trim();
  const lang = body.lang === 'th' || body.lang === 'de' ? body.lang : 'en';
  if (!question) return NextResponse.json({ ok: false, error: 'question required' }, { status: 400 });
  const r = await askSql({ question, lang });
  if (!r) return NextResponse.json({ ok: false, error: 'AI could not generate SQL' }, { status: 502 });
  return NextResponse.json({ ok: true, ...r });
}