import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { loadExecutiveBrief } from '@/server/execBrief';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const guard = await apiGuard(req, { perm: 'tile:cockpit:view::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor;
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });
  const brief = await loadExecutiveBrief(actor);
  return NextResponse.json({ ok: true, brief });
}
