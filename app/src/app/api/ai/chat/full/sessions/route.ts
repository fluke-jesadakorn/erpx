import { NextResponse } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { createSession, listSessions } from '@/lib/chat/history';
import { DEFAULT_CHAT_MODEL } from '@folio-lib/ai/defaults';

export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const g = await apiGuard(req, { perm: 'ai:chat:full::allow' });
  if (g.response) return g.response;
  const sessions = await listSessions(g.actor!.id);
  return NextResponse.json({ sessions });
}

export async function POST(req: Request) {
  const g = await apiGuard(req, { perm: 'ai:chat:full::allow' });
  if (g.response) return g.response;
  const body = await req.json().catch(() => ({}));
  const session = await createSession(g.actor!.id, body.title, body.model || DEFAULT_CHAT_MODEL);
  return NextResponse.json({ session });
}