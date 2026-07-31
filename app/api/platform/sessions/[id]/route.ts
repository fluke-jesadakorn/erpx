import { NextResponse } from 'next/server';
import { platformApiGuard, PLATFORM_SUPPORT } from '@/platform/access';
import { isUuid, safeApiError } from '@/platform/input';
import { revokeSupportSession } from '@/platform/queries';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface RouteProps {
  params: Promise<{ id: string }>;
}

export async function DELETE(req: Request, { params }: RouteProps) {
  const guard = await platformApiGuard(req, PLATFORM_SUPPORT);
  if (guard.response) return guard.response;
  const { id } = await params;
  if (!isUuid(id)) return NextResponse.json({ ok: false, error: 'invalid session id' }, { status: 400 });
  try {
    await revokeSupportSession(id, `${guard.actor.fullname} (${guard.actor.employee_code})`);
    return NextResponse.json({ ok: true });
  } catch (error) {
    return NextResponse.json({ ok: false, error: safeApiError(error, 'support_session_revoke_failed') }, { status: 400 });
  }
}
