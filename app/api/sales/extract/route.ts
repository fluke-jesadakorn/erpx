import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { extractSoFromText } from '@/sales/extract';

export const dynamic = 'force-dynamic';

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: 'tile:sales:view::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor;
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  const body = await req.json().catch(() => null);
  if (!body || typeof body.text !== 'string') {
    return NextResponse.json({ ok: false, error: 'missing text' }, { status: 400 });
  }
  const lang = (body.lang ?? 'en') as 'en' | 'th' | 'de';
  const draft = await extractSoFromText(body.text, lang);
  if (!draft) return NextResponse.json({ ok: true, draft: null });
  return NextResponse.json({ ok: true, draft });
}
