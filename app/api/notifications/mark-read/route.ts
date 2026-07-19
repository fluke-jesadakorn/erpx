import { NextResponse } from 'next/server';
import { requireActor } from '@/server/guard';
import { setReadState } from '@/notifications/queries';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface MarkReadBody {
  ids?: Array<string | number>;
  all?: boolean;
  read?: boolean;
}

function parseIds(raw: unknown): number[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .map((value) => Number(value))
    .filter((value) => Number.isInteger(value) && value > 0);
}

export async function POST(req: Request) {
  const actor = await requireActor().catch(() => null);
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });

  const body = (await req.json().catch(() => ({}))) as MarkReadBody;
  const result = await setReadState(
    actor.id,
    parseIds(body.ids),
    body.read === false ? 'unmark' : 'mark',
    body.all === true,
  );
  return NextResponse.json({ ok: true, updated: result.updated });
}
