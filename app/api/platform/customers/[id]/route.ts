import { NextResponse } from 'next/server';
import { platformApiGuard, PLATFORM_VIEW } from '@/platform/access';
import { isUuid } from '@/platform/input';
import { getPlatformCustomer, recordGlobalRead } from '@/platform/queries';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface RouteProps {
  params: Promise<{ id: string }>;
}

export async function GET(req: Request, { params }: RouteProps) {
  const guard = await platformApiGuard(req, PLATFORM_VIEW);
  if (guard.response) return guard.response;
  const { id } = await params;
  if (!isUuid(id)) return NextResponse.json({ ok: false, error: 'invalid customer id' }, { status: 400 });
  const detail = await getPlatformCustomer(id);
  if (!detail) return NextResponse.json({ ok: false, error: 'customer_not_found' }, { status: 404 });
  const correlationId = await recordGlobalRead({
    customerId: id,
    actorId: guard.actor.id,
    moduleKey: 'customer_360',
    purpose: 'platform_customer_console',
  });
  return NextResponse.json({ ok: true, detail, audit: { correlationId } });
}
