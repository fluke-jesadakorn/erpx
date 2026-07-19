import { NextRequest, NextResponse } from 'next/server';
import { runApproverNudge } from '@/waybill/nudge';
import { isTrustedWorkerRequest } from '@/server/internalAuth';

export const dynamic = 'force-dynamic';
export const runtime = 'nodejs';

export async function POST(req: NextRequest) {
  if (!isTrustedWorkerRequest(req)) {
    return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });
  }
  const rawLang = req.nextUrl.searchParams.get('lang') ?? 'en';
  if (!['en', 'th', 'de'].includes(rawLang)) {
    return NextResponse.json({ ok: false, error: 'invalid language' }, { status: 400 });
  }
  const lang = rawLang as 'en' | 'th' | 'de';
  const sent = await runApproverNudge(lang);
  return NextResponse.json({ ok: true, count: sent.length });
}
