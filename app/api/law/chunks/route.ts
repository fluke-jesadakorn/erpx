import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { listChunks } from '@/law/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const guard = await apiGuard(req);
  if (guard.response) return guard.response;
  const url = new URL(req.url);
  const id = url.searchParams.get('contractId') || url.searchParams.get('id');
  if (!id) return NextResponse.json({ ok: false, error: 'contractId is required' }, { status: 400 });
  try {
    const chunks = await listChunks(id);
    return NextResponse.json({ ok: true, chunks });
  } catch (err) {
    return NextResponse.json({ ok: false, error: (err as Error).message }, { status: 500 });
  }
}
