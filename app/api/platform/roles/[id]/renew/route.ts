import { NextResponse } from 'next/server';
import { platformApiGuard, PLATFORM_RBAC } from '@/platform/access';
import { isUuid, objectBody, safeApiError } from '@/platform/input';
import { renewRoleAssignment } from '@/platform/rbac';
import { isStepUpVerificationError, STEP_UP_OPERATIONS, verifyStepUpEvidence } from '@/platform/step-up';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface RouteProps { params: Promise<{ id: string }> }

export async function POST(req: Request, { params }: RouteProps) {
  const guard = await platformApiGuard(req, PLATFORM_RBAC);
  if (guard.response) return guard.response;
  const { id } = await params;
  if (!isUuid(id)) return NextResponse.json({ ok: false, error: 'invalid assignment id' }, { status: 400 });
  const body = objectBody(await req.json().catch(() => null));
  const durationMinutes = Math.floor(Number(body.durationMinutes ?? 0));
  const validFrom = new Date();
  const validUntil = new Date(validFrom.getTime() + durationMinutes * 60000);
  if (!Number.isFinite(durationMinutes) || durationMinutes < 1) return NextResponse.json({ ok: false, error: 'durationMinutes is required' }, { status: 400 });
  try {
    const stepUpToken = req.headers.get('x-erpx-step-up-evidence');
    const stepUpEvidence = stepUpToken
      ? verifyStepUpEvidence(stepUpToken, { actorId: guard.actor.id, action: STEP_UP_OPERATIONS.renewRole })
      : undefined;
    const assignment = await renewRoleAssignment({
      id,
      validFrom,
      validUntil,
      grantor: `${guard.actor.fullname} (${guard.actor.employee_code})`,
      grantorActorId: guard.actor.id,
      stepUpEvidence,
    });
    return NextResponse.json({ ok: true, assignment }, { status: 201 });
  } catch (error) {
    if (isStepUpVerificationError(error)) return NextResponse.json({ ok: false, error: error.code }, { status: 403 });
    return NextResponse.json({ ok: false, error: safeApiError(error, 'role_renewal_failed') }, { status: 400 });
  }
}
