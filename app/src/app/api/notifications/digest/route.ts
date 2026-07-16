import { NextResponse } from 'next/server';
import { requireActor } from '@folio-lib/server/guard';
import {
  generateDigest,
  saveDigest,
  loadLatestDigest,
} from '@folio-lib/ai/digest';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const actor = await requireActor().catch(() => null);
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  const langParam = new URL(req.url).searchParams.get('lang');
  const lang: 'en' | 'th' | 'de' =
    langParam === 'th' || langParam === 'de' ? langParam : 'en';

  const cached = await loadLatestDigest(actor.id);
  if (cached) return NextResponse.json({ ok: true, digest: cached, cached: true });

  const digest = await generateDigest(actor.id, { lang });
  if (!digest) return NextResponse.json({ ok: true, digest: null });

  const end = new Date();
  const start = new Date(end.getTime() - 24 * 60 * 60 * 1000);
  await saveDigest(actor.id, start, end, digest);
  return NextResponse.json({ ok: true, digest, cached: false });
}