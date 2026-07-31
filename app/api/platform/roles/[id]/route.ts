import { NextResponse } from 'next/server';
import { platformApiGuard, PLATFORM_RBAC } from '@/platform/access';
import { isUuid, safeApiError } from '@/platform/input';
import { revokeRoleAssignment } from '@/platform/rbac';
import { isStepUpVerificationError, STEP_UP_OPERATIONS, verifyStepUpEvidence } from '@/platform/step-up';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface RouteProps { params: Promise<{ id: string }> }

export async function DELETE(req: Request, { params }: RouteProps) {
  const guard = await platformApiGuard(req, PLATFORM_RBAC);
  if (guard.response) return guard.response;
  const { id } = await params;
  if (!isUuid(id)) return NextResponse.json({ ok: false, error: 'invalid assignment id' }, { status: 400 });
  try {
    // The persistence boundary checks the assignment risk tier before accepting the revocation.
    // The header is intentionally required here; a high-risk row will fail closed if the
    // evidence is absent or belongs to another operation/actor.
    const stepUpEvidence = req.headers.get('x-erpx-step-up-evidence')
      ? verifyStepUpEvidence(req.headers.get('x-erpx-step-up-evidence'), {
        actorId: guard.actor.id,
        action: STEP_UP_OPERATIONS.revokeRole,
      })
      : undefined;
    const revoked = await revokeRoleAssignment({
      id,
      revokedBy: `${guard.actor.fullname} (${guard.actor.employee_code})`,
      revokedByActorId: guard.actor.id,
      stepUpEvidence,
    });
    return NextResponse.json({ ok: true, revoked });
  } catch (error) {
    if (isStepUpVerificationError(error)) return NextResponse.json({ ok: false, error: error.code }, { status: 403 });
    return NextResponse.json({ ok: false, error: safeApiError(error, 'role_revocation_failed') }, { status: 400 });
  }
}
