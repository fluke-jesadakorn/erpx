import { NextResponse } from 'next/server';
import { platformApiGuard, PLATFORM_VIEW } from '@/platform/access';
import { boundedSearch } from '@/platform/input';
import { listPlatformCustomers } from '@/platform/queries';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const guard = await platformApiGuard(req, PLATFORM_VIEW);
  if (guard.response) return guard.response;
  const url = new URL(req.url);
  const rawSearch = url.searchParams.get('q');
  const search = boundedSearch(rawSearch);
  if (rawSearch && rawSearch.trim() && !search) return NextResponse.json({ ok: false, error: 'search is too long' }, { status: 400 });
  const risk = url.searchParams.get('risk');
  if (risk && risk !== 'all' && risk !== 'roles' && risk !== 'support') return NextResponse.json({ ok: false, error: 'invalid risk filter' }, { status: 400 });
  const customers = await listPlatformCustomers({
    search,
    subscription: url.searchParams.get('subscription') || undefined,
    health: url.searchParams.get('health') || undefined,
    risk: risk === 'roles' || risk === 'support' ? risk : '',
  });
  return NextResponse.json({ ok: true, customers });
}
