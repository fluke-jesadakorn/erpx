import { NextRequest, NextResponse } from 'next/server';
import { generateCommentary } from '@folio-lib/ledger/commentary';

export const dynamic = 'force-dynamic';

export async function GET(req: NextRequest) {
  const code = req.nextUrl.searchParams.get('code');
  if (!code) return NextResponse.json({ ok: false, error: 'missing code' }, { status: 400 });
  const lang = (req.nextUrl.searchParams.get('lang') ?? 'en') as 'en' | 'th' | 'de';
  const periodLabel = req.nextUrl.searchParams.get('period') ?? undefined;
  const c = await generateCommentary({ accountCode: code, lang, periodLabel });
  if (!c) return NextResponse.json({ ok: true, commentary: null });
  return NextResponse.json({ ok: true, commentary: c });
}