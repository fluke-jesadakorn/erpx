import { NextRequest, NextResponse } from 'next/server';
import { generateReviewHint, type ReviewStage } from '@folio-lib/waybill/reviewHint';

export const dynamic = 'force-dynamic';

export async function POST(req: NextRequest, ctx: { params: Promise<{ id: string }> }) {
  const { id } = await ctx.params;
  const body = await req.json().catch(() => null);
  const stage = (body?.stage ?? 'hod') as ReviewStage;
  if (stage !== 'hod' && stage !== 'am') {
    return NextResponse.json({ ok: false, error: 'stage must be hod|am' }, { status: 400 });
  }
  const lang = (body.lang ?? 'en') as 'en' | 'th' | 'de';
  const hint = await generateReviewHint({ waybillId: id, stage, lang });
  if (!hint) return NextResponse.json({ ok: false, error: 'AI hint unavailable' }, { status: 502 });
  return NextResponse.json({ ok: true, hint });
}