import { NextRequest, NextResponse } from 'next/server';
import { helpWithExpenseSubmit } from '@folio-lib/waybill/expenseHelper';

export const dynamic = 'force-dynamic';

export async function POST(req: NextRequest) {
  const body = await req.json().catch(() => null);
  const text = String(body?.text ?? '').trim();
  const submitterId = Number(body?.submitterId);
  if (!text || !Number.isFinite(submitterId)) {
    return NextResponse.json({ ok: false, error: 'missing text or submitterId' }, { status: 400 });
  }
  const lang = (body.lang ?? 'en') as 'en' | 'th' | 'de';
  const hint = await helpWithExpenseSubmit({ rawText: text, submitterId, lang });
  if (!hint) return NextResponse.json({ ok: true, hint: null });
  return NextResponse.json({ ok: true, hint });
}