import { NextRequest, NextResponse } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { listRecentNudgesForUser, runApproverNudge } from '@folio-lib/waybill/nudge';

export const dynamic = 'force-dynamic';

export async function GET(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'tile:inbox:view::allow' });
  if (guard.response) return guard.response;
  const limit = Math.min(Math.max(Number(req.nextUrl.searchParams.get('limit') ?? '10'), 1), 50);
  const actor = guard.actor as { id: number };
  const items = await listRecentNudgesForUser(actor.id, limit);
  return NextResponse.json({ ok: true, items });
}

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'tile:inbox:view::allow' });
  if (guard.response) return guard.response;
  const body = await req.json().catch(() => null);
  const lang = (body?.lang ?? 'en') as 'en' | 'th' | 'de';
  const sent = await runApproverNudge(lang);
  return NextResponse.json({ ok: true, sent });
}