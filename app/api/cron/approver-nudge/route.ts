import { NextRequest, NextResponse } from 'next/server';
import { runApproverNudge } from '@/waybill/nudge';

export const dynamic = 'force-dynamic';
export const runtime = 'nodejs';

export async function POST(req: NextRequest) {
  const lang = (req.nextUrl.searchParams.get('lang') ?? 'en') as 'en' | 'th' | 'de';
  const sent = await runApproverNudge(lang);
  return NextResponse.json({ ok: true, count: sent.length, nudges: sent });
}