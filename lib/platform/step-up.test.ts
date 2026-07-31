import assert from 'node:assert/strict';
import { createHmac, randomUUID } from 'node:crypto';
import test from 'node:test';
import {
  assertStepUpEvidence,
  isStepUpVerificationError,
  STEP_UP_OPERATIONS,
  StepUpVerificationError,
  verifyStepUpEvidence,
} from './step-up';

const SECRET = 'platform-step-up-test-secret-012345678901234567890123456789';

function issueTestStepUpEvidence(args: {
  actorId: number;
  action: typeof STEP_UP_OPERATIONS[keyof typeof STEP_UP_OPERATIONS];
  approverRef: string;
  issuedAt?: Date;
}): string {
  const payload = Buffer.from(JSON.stringify({
    version: 1,
    actorId: args.actorId,
    action: args.action,
    assurance: 'mfa',
    issuer: 'test-mfa-authority',
    evidenceId: randomUUID(),
    approverRef: args.approverRef,
    issuedAt: Math.floor((args.issuedAt?.getTime() ?? Date.now()) / 1000),
  }), 'utf8').toString('base64url');
  return `${payload}.${createHmac('sha256', SECRET).update(payload).digest('base64url')}`;
}

function withSecret<T>(fn: () => T): T {
  const previous = process.env.ERPX_PLATFORM_STEP_UP_SECRET;
  process.env.ERPX_PLATFORM_STEP_UP_SECRET = SECRET;
  try {
    return fn();
  } finally {
    if (previous === undefined) delete process.env.ERPX_PLATFORM_STEP_UP_SECRET;
    else process.env.ERPX_PLATFORM_STEP_UP_SECRET = previous;
  }
}

test('step-up evidence is signed, actor-bound, operation-bound, and fresh', () => {
  withSecret(() => {
    const token = issueTestStepUpEvidence({ actorId: 41, action: STEP_UP_OPERATIONS.createRole, approverRef: 'security-head-9' });
    const evidence = verifyStepUpEvidence(token, { actorId: 41, action: STEP_UP_OPERATIONS.createRole });
    assert.equal(evidence.assurance, 'mfa');
    assert.equal(evidence.approverRef, 'security-head-9');
    assert.equal(evidence.actorId, 41);
  });
});

test('step-up evidence fails closed for tampering, wrong actor, wrong operation, and expiry', () => {
  withSecret(() => {
    const token = issueTestStepUpEvidence({ actorId: 41, action: STEP_UP_OPERATIONS.renewRole, approverRef: 'security-head-9' });
    assert.throws(
      () => verifyStepUpEvidence(`${token.slice(0, -1)}x`, { actorId: 41, action: STEP_UP_OPERATIONS.renewRole }),
      (error) => isStepUpVerificationError(error),
    );
    assert.throws(() => verifyStepUpEvidence(token, { actorId: 42, action: STEP_UP_OPERATIONS.renewRole }), StepUpVerificationError);
    assert.throws(() => verifyStepUpEvidence(token, { actorId: 41, action: STEP_UP_OPERATIONS.createRole }), StepUpVerificationError);
    const expired = issueTestStepUpEvidence({
      actorId: 41,
      action: STEP_UP_OPERATIONS.renewRole,
      approverRef: 'security-head-9',
      issuedAt: new Date(Date.now() - 6 * 60 * 1000),
    });
    assert.throws(() => verifyStepUpEvidence(expired, { actorId: 41, action: STEP_UP_OPERATIONS.renewRole }), /expired/);
  });
});

test('persistence boundary rejects missing or self-approved high-risk evidence', () => {
  withSecret(() => {
    assert.throws(
      () => assertStepUpEvidence(undefined, { actorId: 41, action: STEP_UP_OPERATIONS.createRole }),
      /step-up MFA evidence is required/,
    );
    const selfApproved = verifyStepUpEvidence(
      issueTestStepUpEvidence({ actorId: 41, action: STEP_UP_OPERATIONS.createRole, approverRef: '41' }),
      { actorId: 41, action: STEP_UP_OPERATIONS.createRole },
    );
    assert.throws(
      () => assertStepUpEvidence(selfApproved, { actorId: 41, action: STEP_UP_OPERATIONS.createRole }),
      /independent approver/,
    );
  });
});
