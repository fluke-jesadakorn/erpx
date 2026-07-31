import { NextResponse } from 'next/server';
import { platformApiGuard, PLATFORM_VIEW } from '@/platform/access';
import { loadFolioBridgeReadStatus } from '@/platform/bridge';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const guard = await platformApiGuard(req, PLATFORM_VIEW);
  if (guard.response) return guard.response;
  const bridge = await loadFolioBridgeReadStatus();
  return NextResponse.json(
    { ok: true, bridge },
    { headers: { 'cache-control': 'no-store, max-age=0' } },
  );
}
