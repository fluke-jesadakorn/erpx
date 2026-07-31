import { createHmac, timingSafeEqual } from 'node:crypto';

export const STEP_UP_MAX_AGE_SECONDS = 5 * 60;
const STEP_UP_CLOCK_SKEW_SECONDS = 30;
const VERIFIED_STEP_UP_BRAND = Symbol('verified-step-up-evidence');

export const STEP_UP_OPERATIONS = {
  createRole: 'platform.role.create',
  renewRole: 'platform.role.renew',
  revokeRole: 'platform.role.revoke',
} as const;

export type StepUpOperation = typeof STEP_UP_OPERATIONS[keyof typeof STEP_UP_OPERATIONS];

export interface StepUpEvidence {
  readonly [VERIFIED_STEP_UP_BRAND]: true;
  version: 1;
  actorId: number;
  action: StepUpOperation;
  assurance: 'mfa';
  issuer: string;
  evidenceId: string;
  approverRef: string;
  issuedAt: number;
}

export class StepUpVerificationError extends Error {
  readonly code = 'step_up_mfa_required';

  constructor(message = 'server-verified step-up MFA evidence is required') {
    super(message);
    this.name = 'StepUpVerificationError';
  }
}

function secret(): string {
  const value = process.env.ERPX_PLATFORM_STEP_UP_SECRET?.trim();
  if (!value || value.length < 32) throw new StepUpVerificationError('step-up MFA verification is not configured');
  return value;
}

function decode(value: string): string {
  try {
    return Buffer.from(value, 'base64url').toString('utf8');
  } catch {
    throw new StepUpVerificationError();
  }
}

function signature(payload: string): Buffer {
  return createHmac('sha256', secret()).update(payload).digest();
}

function isOperation(value: unknown): value is StepUpOperation {
  return value === STEP_UP_OPERATIONS.createRole
    || value === STEP_UP_OPERATIONS.renewRole
    || value === STEP_UP_OPERATIONS.revokeRole;
}

function parseEvidence(payload: string): StepUpEvidence {
  let value: unknown;
  try {
    value = JSON.parse(decode(payload));
  } catch {
    throw new StepUpVerificationError();
  }
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new StepUpVerificationError();
  const candidate = value as Record<string, unknown>;
  const actorId = candidate.actorId;
  const issuedAt = candidate.issuedAt;
  const issuer = typeof candidate.issuer === 'string' ? candidate.issuer.trim() : '';
  const evidenceId = typeof candidate.evidenceId === 'string' ? candidate.evidenceId.trim() : '';
  const approverRef = typeof candidate.approverRef === 'string' ? candidate.approverRef.trim() : '';
  if (candidate.version !== 1 || !Number.isInteger(actorId) || Number(actorId) <= 0 || !isOperation(candidate.action)
    || candidate.assurance !== 'mfa' || !issuer || issuer.length > 120 || !evidenceId || evidenceId.length > 160
    || !approverRef || approverRef.length > 200 || !Number.isInteger(issuedAt)) {
    throw new StepUpVerificationError();
  }
  return {
    [VERIFIED_STEP_UP_BRAND]: true,
    version: 1,
    actorId: Number(actorId),
    action: candidate.action,
    assurance: 'mfa',
    issuer,
    evidenceId,
    approverRef,
    issuedAt: Number(issuedAt),
  };
}

function assertFresh(evidence: StepUpEvidence, nowSeconds: number): void {
  const age = nowSeconds - evidence.issuedAt;
  if (age < -STEP_UP_CLOCK_SKEW_SECONDS || age > STEP_UP_MAX_AGE_SECONDS) {
    throw new StepUpVerificationError('step-up MFA evidence is missing or expired');
  }
}

/** Verify a short-lived, MFA-backed assertion issued by the configured step-up authority. */
export function verifyStepUpEvidence(
  token: string | null | undefined,
  expected: { actorId: number; action: StepUpOperation; now?: Date },
): StepUpEvidence {
  if (!token) throw new StepUpVerificationError();
  const parts = token.trim().split('.');
  if (parts.length !== 2 || !parts[0] || !parts[1]) throw new StepUpVerificationError();
  const payload = parts[0];
  let supplied: Buffer;
  try {
    supplied = Buffer.from(parts[1], 'base64url');
  } catch {
    throw new StepUpVerificationError();
  }
  const expectedSignature = signature(payload);
  if (supplied.length !== expectedSignature.length || !timingSafeEqual(supplied, expectedSignature)) {
    throw new StepUpVerificationError();
  }
  const evidence = parseEvidence(payload);
  assertFresh(evidence, Math.floor((expected.now?.getTime() ?? Date.now()) / 1000));
  if (evidence.actorId !== expected.actorId || evidence.action !== expected.action) throw new StepUpVerificationError();
  return evidence;
}

/** Re-check evidence at the persistence boundary so direct library callers cannot bypass MFA. */
export function assertStepUpEvidence(
  evidence: StepUpEvidence | null | undefined,
  expected: { actorId: number; action: StepUpOperation; now?: Date },
): StepUpEvidence {
  if (!evidence || evidence[VERIFIED_STEP_UP_BRAND] !== true || evidence.actorId !== expected.actorId || evidence.action !== expected.action || evidence.assurance !== 'mfa') {
    throw new StepUpVerificationError();
  }
  assertFresh(evidence, Math.floor((expected.now?.getTime() ?? Date.now()) / 1000));
  if (evidence.approverRef.toLowerCase() === String(expected.actorId)) {
    throw new StepUpVerificationError('high-risk role operations require an independent approver');
  }
  return evidence;
}

export function isStepUpVerificationError(error: unknown): error is StepUpVerificationError {
  return error instanceof StepUpVerificationError;
}
