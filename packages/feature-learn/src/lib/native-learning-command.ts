import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import { holdingResources } from '@erpx/server-runtime/public/runtime';
import { learningPurchaseControls } from '@erpx/server-learning/public/purchases';
import type { LearningPurchaseControls } from '@erpx/server-learning/public/purchases';
import {
  createNativeLearningClient,
  nativeLearningAvailable,
  nativeLearningConfiguration,
  nativeLearningFailure,
  NativeLearningError,
  type NativeLearningPageContext,
  type NativeLearningReadInput,
  type NativeLearningReceipt,
} from '@erpx/server-learning/public/native-learning';

type Input = Record<string, unknown>;

function pageContext(actor: WorkContext): NativeLearningPageContext {
  if (!actor.authority || !actor.iam)
    throw new NativeLearningError('NATIVE_LEARNING_CONTEXT_CHANGED');
  return {
    userId: actor.userId,
    holdingId: actor.holdingId,
    holdingSlug: actor.holdingSlug,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    assignmentId: actor.assignmentId,
    role: actor.role,
    assignmentScope: actor.assignmentScope,
    scope: actor.scope,
    authority: actor.authority,
    modules: actor.modules,
    iam: actor.iam,
  };
}

function readInput(context: NativeLearningPageContext): NativeLearningReadInput {
  return {
    holdingSlug: context.holdingSlug,
    branchSlug: context.scope.kind === 'holding' ? undefined : context.scope.branchSlug,
    departmentSlug:
      context.scope.kind === 'department' ? context.scope.departmentSlug : undefined,
    expected: {
      companyId: context.companyId,
      assignmentId: context.assignmentId,
      revision: context.authority!.revision,
      assignmentVersion: context.authority!.assignmentVersion,
    },
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_LEARNING_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeLearningClient(nativeLearningConfiguration(raw));
}

function invalid(): never {
  throw new ApiError(422, 'LEARNING_REQUEST_INVALID', 'Review the Learning request.');
}

function string(value: unknown): string {
  if (typeof value !== 'string' || value.length < 1 || value.length > 200) invalid();
  return value;
}

function text(value: unknown, maximum: number): string {
  if (
    typeof value !== 'string' ||
    value.trim().length < 1 ||
    new TextEncoder().encode(value).length > maximum ||
    /[\u0000-\u001f\u007f]/.test(value)
  )
    invalid();
  return value;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function integer(value: unknown, minimum = 1): number {
  if (!Number.isSafeInteger(value) || (value as number) < minimum) invalid();
  return value as number;
}

function requirePurchase(
  controls: LearningPurchaseControls | undefined,
  capability: keyof LearningPurchaseControls,
  purpose: 'newWork' | 'settleExisting' | 'history',
) {
  if (controls && controls[capability]?.[purpose] !== true)
    throw new ApiError(
      403,
      'LEARNING_PURCHASE_REQUIRED',
      'Your current Company package does not include this Learning command.',
    );
}

function courseAction(value: unknown): 'publish' | 'reject' | 'retire' {
  if (value === 'publish' || value === 'reject' || value === 'retire') return value;
  return invalid();
}

function assignmentAction(value: unknown): 'start' | 'complete' | 'waive' | 'cancel' {
  if (value === 'start' || value === 'complete' || value === 'waive' || value === 'cancel')
    return value;
  return invalid();
}

function attachmentIds(value: unknown): string[] {
  if (value === undefined) return [];
  if (!Array.isArray(value) || value.length > 20 || !value.every((item) => typeof item === 'string'))
    return invalid();
  return value as string[];
}

/**
 * Native Learning command cutover for the retained `/learning` POST route.
 * Returns undefined only when configuration is unset. Once configured, every
 * owned course/assignment/certificate command uses the native owner and fails
 * closed; an attached completion is explicitly unavailable rather than a
 * silent legacy fallback.
 */
export async function nativeLearningCommandRoute(
  actor: WorkContext,
  input: Input,
  key: string,
  incoming: Pick<Headers, 'get'>,
): Promise<NativeLearningReceipt | undefined> {
  if (!nativeLearningAvailable()) return undefined;
  try {
    const native = client();
    if (!native) return undefined;
    const context = pageContext(actor);
    const read = readInput(context);
    const entity = input.entity;
    const write = (await holdingResources(actor.holdingId)).write;
    const controls = await learningPurchaseControls(write, actor);
    const id = optionalString(input.id);

    if (entity === 'course') {
      if (id === undefined) {
        requirePurchase(controls, 'learn.courses', 'newWork');
        const body = JSON.stringify({
          title: text(input.title, 160),
          description: text(input.description, 3_000),
          provider: text(input.provider, 160),
          durationMinutes: integer(input.durationMinutes),
          requiredFor:
            typeof input.requiredFor === 'string' && input.requiredFor.trim()
              ? input.requiredFor
              : null,
        });
        return await native.createCourse(incoming, context, body, key, read);
      }
      const action = courseAction(input.action);
      requirePurchase(
        controls,
        'learn.courses',
        action === 'publish' ? 'newWork' : 'settleExisting',
      );
      const body = JSON.stringify({
        action,
        expectedVersion: integer(input.expectedVersion),
        reason: text(input.reason, 3_000),
      });
      return await native.changeCourse(incoming, context, id, body, key, read);
    }

    if (entity === 'certificate') {
      if (id === undefined) return invalid();
      requirePurchase(controls, 'learn.certificates', 'settleExisting');
      const body = JSON.stringify({
        expectedAssignmentVersion: integer(input.expectedAssignmentVersion),
        reason: text(input.reason, 2_000),
      });
      return await native.revokeCertificate(incoming, context, id, body, key, read);
    }

    if (id === undefined) {
      requirePurchase(controls, 'learn.assignments', 'newWork');
      requirePurchase(controls, 'learn.courses', 'newWork');
      requirePurchase(controls, 'people.learner-reference', 'newWork');
      const body = JSON.stringify({
        courseId: string(input.courseId),
        courseVersion: integer(input.courseVersion),
        learnerId: string(input.learnerId),
        dueDate: string(input.dueDate),
        reason: text(input.reason, 3_000),
      });
      return await native.createAssignment(incoming, context, body, key, read);
    }

    const action = assignmentAction(input.action);
    const attachments = attachmentIds(input.attachmentSetIds);
    if (action === 'complete' && attachments.length > 0)
      throw new ApiError(
        422,
        'LEARNING_ATTACHMENTS_UNAVAILABLE',
        'Native Learning completion does not yet bind private attachments.',
      );
    requirePurchase(controls, 'learn.assignments', 'settleExisting');
    if (action === 'complete' || action === 'waive') {
      requirePurchase(controls, 'learn.results', 'settleExisting');
      const detail = await native.detail(incoming, context, id, read);
      if (detail.assignment.required_for) {
        requirePurchase(controls, 'learn.certificates', 'settleExisting');
      }
    }
    const expectedVersion = integer(input.expectedVersion);
    const body =
      action === 'complete'
        ? JSON.stringify({
            action,
            expectedVersion,
            reason: text(input.reason, 3_000),
            attendance: string(input.attendance),
            scoreBps: integer(input.scoreBps, 0),
            evidence: text(input.evidence, 3_000),
            ...(typeof input.certificateValidFrom === 'string'
              ? { certificateValidFrom: input.certificateValidFrom }
              : {}),
            certificateExpiresOn:
              typeof input.certificateExpiresOn === 'string'
                ? input.certificateExpiresOn
                : null,
            attachmentSetIds: [],
          })
        : JSON.stringify({
            action,
            expectedVersion,
            reason: text(input.reason, 3_000),
          });
    return await native.changeAssignment(incoming, context, id, body, key, read);
  } catch (error) {
    const failure = nativeLearningFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}
