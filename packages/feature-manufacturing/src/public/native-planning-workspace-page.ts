import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  createNativePlanningClient,
  nativePlanningAvailable,
  nativePlanningConfiguration,
  NativePlanningError,
  type NativePlanningPageContext,
} from './native-planning-workspace';
import type { NativePlanningWorkspace } from '../lib/native-planning-types';
import { createNextNativePlanningWorkspaceReader } from '../adapters/server/next-native-planning-workspace';

export { nativePlanningAvailable };
export type { NativePlanningPageContext } from './native-planning-workspace';
export type {
  NativePlanningAllocation,
  NativePlanningCalendarDay,
  NativePlanningContext,
  NativePlanningRecommendation,
  NativePlanningResource,
  NativePlanningScenario,
  NativePlanningShortage,
  NativePlanningWorkspace,
} from '../lib/native-planning-types';

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativePlanningPageContext(actor: WorkContext): NativePlanningPageContext {
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

function expected(actor: NativePlanningPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam)
    throw new NativePlanningError('NATIVE_PLANNING_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativePlanningPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_PLANNING_CONFIG;
  if (raw === undefined) return undefined;
  return createNativePlanningClient(nativePlanningConfiguration(raw));
}

export async function nativePlanningWorkspace(
  actor: NativePlanningPageContext,
  holdingSlug: string,
): Promise<NativePlanningWorkspace | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativePlanningError('NATIVE_PLANNING_CONTEXT_CHANGED');
  return createNextNativePlanningWorkspaceReader((incoming, input, signal) =>
    native.workspace(incoming, actor, input, signal),
  )({ holdingSlug, ...scope(actor), expected: expected(actor) });
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativePlanningFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativePlanningError)) return undefined;
  if (error.code === 'NATIVE_PLANNING_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return {
      status: 403,
      code: error.code === 'NATIVE_PLANNING_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return { status: 404, code: 'NOT_FOUND', message: 'Planning source not found in this scope.' };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_PLANNING_UNAVAILABLE',
      message: 'The native Planning service is unavailable.',
    };
  if (error.code === 'NATIVE_PLANNING_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_PLANNING_UNAVAILABLE',
      message: 'The native Planning reader is unavailable.',
    };
  if (error.code === 'NATIVE_PLANNING_REQUEST_INVALID')
    return {
      status: 422,
      code: 'PLANNING_REQUEST_INVALID',
      message: 'Review the Planning request.',
    };
  if (
    error.code === 'NATIVE_PLANNING_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_PLANNING_COOKIE_LIMIT' ||
    error.code === 'NATIVE_PLANNING_RESPONSE_INVALID' ||
    error.code === 'NATIVE_PLANNING_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_PLANNING_UNAVAILABLE',
      message: 'The native Planning reader is unavailable.',
    };
  return undefined;
}
