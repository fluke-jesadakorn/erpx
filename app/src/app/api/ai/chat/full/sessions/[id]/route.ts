import { NextResponse } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { deleteSession, loadSession } from '@/lib/chat/history';

export const dynamic = 'force-dynamic';

export async function GET(req: Request, { params }: { params: Promise<{ id: string }> }) {
  const g = await apiGuard(req, { perm: 'ai:chat:full::allow' });
  if (g.response) return g.response;
  const { id } = await params;
  const out = await loadSession(g.actor!.id, id);
  if (!out) return NextResponse.json({ error: 'not found' }, { status: 404 });
  return NextResponse.json(out);
}

export async function DELETE(req: Request, { params }: { params: Promise<{ id: string }> }) {
  const g = await apiGuard(req, { perm: 'ai:chat:full::allow' });
  if (g.response) return g.response;
  const { id } = await params;
  await deleteSession(g.actor!.id, id);
  return NextResponse.json({ ok: true });
}