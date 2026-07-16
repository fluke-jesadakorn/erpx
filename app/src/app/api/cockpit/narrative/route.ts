import { NextResponse } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { getDashboardForRole } from '@folio-lib/dashboard/queries';

export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const guard = await apiGuard(req, { perm: 'tile:cockpit:view::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor;
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  const dash = await getDashboardForRole(actor.id);
  return NextResponse.json({ ok: true, narrative: dash });
}