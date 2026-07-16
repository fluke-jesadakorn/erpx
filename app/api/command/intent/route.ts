import { NextRequest, NextResponse } from 'next/server';
import { classifyIntent } from '@/ai/command';

export const dynamic = 'force-dynamic';

export async function POST(req: NextRequest) {
  const body = await req.json().catch(() => null);
  const q = String(body?.query ?? '').trim();
  if (!q) return NextResponse.json({ ok: false, error: 'empty query' }, { status: 400 });
  const intent = await classifyIntent(q);
  if (!intent) return NextResponse.json({ ok: true, intent: null });
  return NextResponse.json({ ok: true, intent });
}