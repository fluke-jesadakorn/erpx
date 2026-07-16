import { NextResponse } from 'next/server';
import { requireActor } from '@/server/guard';
import { setReadState } from '@/notifications/queries';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface MarkReadBody {
  ids?: Array<string | number>;
  all?: boolean;
}

function parseIds(raw: unknown): number[] {
  if (!Array.isArray(raw)) return [];
  const out: number[] = [];
  for (const v of raw) {
    const n = typeof v === 'string' ? parseInt(v, 10) : typeof v === 'number' ? v : NaN;
    if (!Number.isNaN(n) && n > 0) out.push(n);
  }
  return out;
}

export async function POST(req: Request) {
  const actor = await requireActor().catch(() => null);
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });

  const body = (await req.json().catch(() => ({}))) as MarkReadBody;
  const all = body.all === true;
  const ids = parseIds(body.ids);

  const result = await setReadState(actor.id, ids, 'mark', all);
  return NextResponse.json({ ok: true, updated: result.updated });
}