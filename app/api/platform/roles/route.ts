import { NextResponse } from 'next/server';
import { platformApiGuard, PLATFORM_RBAC } from '@/platform/access';
import { boundedOptionalText, boundedStringArray, boundedText, boundedUuidArray, isUuid, objectBody, safeApiError } from '@/platform/input';
import { createRoleAssignment, listRoleAssignments, type AssignmentRiskTier, type AssignmentSubtreeScope } from '@/platform/rbac';
import { isStepUpVerificationError, STEP_UP_OPERATIONS, verifyStepUpEvidence } from '@/platform/step-up';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const RISK_TIERS = new Set<AssignmentRiskTier>(['platform_root', 'administrator', 'high', 'standard', 'contractor', 'support', 'emergency']);

export async function GET(req: Request) {
  const guard = await platformApiGuard(req, PLATFORM_RBAC);
  if (guard.response) return guard.response;
  return NextResponse.json({ ok: true, assignments: await listRoleAssignments() });
}

export async function POST(req: Request) {
  const guard = await platformApiGuard(req, PLATFORM_RBAC);
  if (guard.response) return guard.response;
  const body = objectBody(await req.json().catch(() => null));
  const riskTier = String(body.riskTier ?? 'standard') as AssignmentRiskTier;
  const principalRef = boundedText(body.principalRef, 200) ?? '';
  const roleKey = boundedText(body.roleKey, 120) ?? '';
  const grantReason = boundedText(body.grantReason, 2000) ?? '';
  const durationMinutes = Math.floor(Number(body.durationMinutes ?? 0));
  if (!RISK_TIERS.has(riskTier) || !principalRef || !roleKey || grantReason.length < 10 || !Number.isFinite(durationMinutes) || durationMinutes < 1) {
    return NextResponse.json({ ok: false, error: 'principal, role, reason, risk tier, and duration are required' }, { status: 400 });
  }
  const holdingId = body.holdingId == null || body.holdingId === '' ? null : String(body.holdingId);
  const companyId = body.companyId == null || body.companyId === '' ? null : String(body.companyId);
  const branchId = body.branchId == null || body.branchId === '' ? null : String(body.branchId);
  const parentAssignmentId = body.parentAssignmentId == null || body.parentAssignmentId === '' ? null : String(body.parentAssignmentId);
  if ((holdingId && !isUuid(holdingId)) || (companyId && !isUuid(companyId)) || (branchId && !isUuid(branchId)) || (parentAssignmentId && !isUuid(parentAssignmentId))) {
    return NextResponse.json({ ok: false, error: 'scope identifiers must be UUIDs' }, { status: 400 });
  }
  const authorityLevel = Number(body.authorityLevel ?? 99);
  if (!Number.isFinite(authorityLevel)) return NextResponse.json({ ok: false, error: 'authorityLevel is invalid' }, { status: 400 });
  const validFrom = new Date();
  const validUntil = new Date(validFrom.getTime() + durationMinutes * 60000);
  try {
    const stepUpEvidence = ['platform_root', 'administrator', 'high'].includes(riskTier)
      ? verifyStepUpEvidence(req.headers.get('x-erpx-step-up-evidence'), { actorId: guard.actor.id, action: STEP_UP_OPERATIONS.createRole })
      : undefined;
    const assignment = await createRoleAssignment({
      principalRef,
      principalType: body.principalType === 'platform_user' || body.principalType === 'service' ? body.principalType : 'tenant_user',
      roleKey,
      permissions: boundedStringArray(body.permissions, 100, 200),
      functionalDomain: boundedText(body.functionalDomain ?? 'organization', 120) ?? 'organization',
      authorityLevel: Math.max(1, Math.min(99, Math.floor(authorityLevel))),
      holdingId,
      companyId,
      branchId,
      subtreeScope: {
        companyIds: boundedUuidArray(objectBody(body.subtreeScope).companyIds),
        branchIds: boundedUuidArray(objectBody(body.subtreeScope).branchIds),
      } satisfies AssignmentSubtreeScope,
      validFrom,
      validUntil,
      timezone: boundedText(body.timezone ?? 'UTC', 80) ?? 'UTC',
      riskTier,
      grantReason,
      ticketReference: boundedOptionalText(body.ticketReference, 200),
      contractReference: boundedOptionalText(body.contractReference, 200),
      grantor: `${guard.actor.fullname} (${guard.actor.employee_code})`,
      grantorActorId: guard.actor.id,
      renewalReviewer: boundedOptionalText(body.renewalReviewer, 200),
      parentAssignmentId,
      stepUpEvidence,
    });
    return NextResponse.json({ ok: true, assignment }, { status: 201 });
  } catch (error) {
    if (isStepUpVerificationError(error)) return NextResponse.json({ ok: false, error: error.code }, { status: 403 });
    return NextResponse.json({ ok: false, error: safeApiError(error, 'role_assignment_failed') }, { status: 400 });
  }
}
