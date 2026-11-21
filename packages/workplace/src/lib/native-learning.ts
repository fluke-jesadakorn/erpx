import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import { holdingResources } from '@erpx/server-runtime/public/runtime';
import {
  learningPurchaseControls,
  requireLearningPurchasedRead,
} from '@erpx/server-learning/public/purchases';
import type { LearningPurchaseControls } from '@erpx/server-learning/public/purchases';
import {
  createNativeLearningClient,
  nativeLearningConfiguration,
  nativeLearningFailure,
  NativeLearningError,
  type NativeLearningAssignment,
  type NativeLearningDetail,
  type NativeLearningPageContext,
  type NativeLearningReadInput,
  type NativeLearningWorkspace,
} from '@erpx/server-learning/public/native-learning';

export { nativeLearningAvailable, nativeLearningFailure } from '@erpx/server-learning/public/native-learning';
export type {
  NativeLearningAssignment,
  NativeLearningCertificate,
  NativeLearningCourse,
  NativeLearningDetail,
  NativeLearningEligibilityCertificate,
  NativeLearningEligibilityConsumption,
  NativeLearningEligibilityFacts,
  NativeLearningEvent,
  NativeLearningHandoff,
  NativeLearningPerson,
  NativeLearningWorkspace,
} from '@erpx/server-learning/public/native-learning';

export type NativeLearningPageWorkspace = NativeLearningWorkspace & {
  purchases?: LearningPurchaseControls;
};
export type NativeLearningPageDetail = NativeLearningDetail & {
  purchases?: LearningPurchaseControls;
};

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

function expected(context: NativeLearningPageContext) {
  if (!context.authority || !context.iam)
    throw new NativeLearningError('NATIVE_LEARNING_CONTEXT_CHANGED');
  return {
    companyId: context.companyId,
    assignmentId: context.assignmentId,
    revision: context.authority.revision,
    assignmentVersion: context.authority.assignmentVersion,
  } as const;
}

function scope(context: NativeLearningPageContext) {
  return {
    branchSlug: context.scope.kind === 'holding' ? undefined : context.scope.branchSlug,
    departmentSlug:
      context.scope.kind === 'department' ? context.scope.departmentSlug : undefined,
  };
}

function client() {
  try {
    const raw = process.env.ERPX_NATIVE_LEARNING_CONFIG;
    if (raw === undefined)
      throw new NativeLearningError('NATIVE_LEARNING_CONFIGURATION_INVALID');
    return createNativeLearningClient(nativeLearningConfiguration(raw));
  } catch (error) {
    const failure = nativeLearningFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

async function purchasePreflight(actor: WorkContext) {
  const write = (await holdingResources(actor.holdingId)).write;
  await requireLearningPurchasedRead(write, actor, 'learn.assignments');
  return learningPurchaseControls(write, actor);
}

function readInput(
  context: NativeLearningPageContext,
  personal = false,
): NativeLearningReadInput {
  return {
    holdingSlug: context.holdingSlug,
    personal,
    ...scope(context),
    expected: expected(context),
  };
}

function redactAssignment(value: NativeLearningAssignment, purchases: LearningPurchaseControls | undefined) {
  return !purchases || purchases['learn.results']?.history === true
    ? value
    : {
        ...value,
        attendance: null,
        score_bps: null,
        result_evidence: null,
        completion_reason: null,
      };
}

export function nativeLearningProjectWorkspace(
  value: NativeLearningWorkspace,
  purchases: LearningPurchaseControls | undefined,
): NativeLearningPageWorkspace {
  const courses =
    !purchases || purchases['learn.courses']?.history === true ? value.courses : [];
  const people =
    !purchases || purchases['people.learner-reference']?.history === true
      ? value.people
      : [];
  return {
    ...value,
    courses,
    people,
    assignments: value.assignments.map((row) => redactAssignment(row, purchases)),
    ...(purchases ? { purchases } : {}),
  };
}

export function nativeLearningProjectDetail(
  value: NativeLearningDetail,
  purchases: LearningPurchaseControls | undefined,
): NativeLearningPageDetail {
  const workspace = nativeLearningProjectWorkspace(value, purchases);
  const history =
    !purchases || purchases['learn.results']?.history === true ? value.history : [];
  const certificates = !purchases || purchases['learn.certificates']?.history === true;
  return {
    ...workspace,
    assignment: redactAssignment(value.assignment, purchases),
    history,
    handoff: certificates ? value.handoff : null,
    certificate: certificates ? value.certificate : null,
  };
}

/**
 * Native Learning workspace read. Callers gate on nativeLearningAvailable();
 * an unset configuration is a bounded failure rather than an implicit
 * TypeScript fallback.
 */
export async function nativeLearningWorkspacePage(
  actor: WorkContext,
  personal = false,
): Promise<NativeLearningPageWorkspace> {
  const native = client();
  const context = pageContext(actor);
  const input = readInput(context, personal);
  const purchases = await purchasePreflight(actor);
  try {
    const value = await native.workspace(await headers(), context, input);
    return nativeLearningProjectWorkspace(value, purchases);
  } catch (error) {
    const failure = nativeLearningFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

export async function nativeLearningDetailPage(
  actor: WorkContext,
  id: string,
): Promise<NativeLearningPageDetail> {
  const native = client();
  const context = pageContext(actor);
  const input = readInput(context);
  const purchases = await purchasePreflight(actor);
  try {
    const value = await native.detail(await headers(), context, id, input);
    return nativeLearningProjectDetail(value, purchases);
  } catch (error) {
    const failure = nativeLearningFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}
