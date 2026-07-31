import { NextResponse } from 'next/server';
import { platformApiGuard, PLATFORM_MANAGE } from '@/platform/access';
import { drainBridgeOutbox } from '@/platform/bridge-write';
import { safeApiError } from '@/platform/input';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: Request) {
  const guard = await platformApiGuard(req, PLATFORM_MANAGE);
  if (guard.response) return guard.response;
  try {
    const body = await req.json().catch(() => ({})) as { limit?: unknown };
    const limit = typeof body.limit === 'number' && Number.isFinite(body.limit) ? body.limit : 100;
    const result = await drainBridgeOutbox(limit);
    return NextResponse.json({ ok: true, bridge: result });
  } catch (error) {
    return NextResponse.json({ ok: false, error: safeApiError(error, 'bridge_drain_failed') }, { status: 409 });
  }
}
