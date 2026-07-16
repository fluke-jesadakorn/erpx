import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { loadOrgTree } from '@/org/scope';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const guard = await apiGuard(req, { perm: 'org:tree:view::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor;
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const tree = await loadOrgTree(actor.id);
  return NextResponse.json({ tree });
}