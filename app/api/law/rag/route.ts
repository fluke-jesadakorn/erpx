import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { PERM } from '@/perm/taxonomy';
import { ask } from '@/law/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
export const maxDuration = 300;

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.law.rag.query });
  if (guard.response) return guard.response;
  const body = (await req.json().catch(() => null)) as { query?: string } | null;
  const query = body?.query?.trim();
  if (!query) return NextResponse.json({ ok: false, error: 'query is required' }, { status: 400 });
  try {
    return NextResponse.json({ ok: true, ...(await ask(query, guard.actor?.id)) });
  } catch (err) {
    return NextResponse.json({ ok: false, error: (err as Error).message }, { status: 502 });
  }
}
