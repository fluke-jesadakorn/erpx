import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  createNativeProjectsClient,
  nativeProjectsAvailable,
  nativeProjectsConfiguration,
  NativeProjectsError,
  type NativeProjectRow,
  type NativeProjectMilestoneRow,
  type NativeProjectWbs,
  type NativeProjectsPageContext,
} from './native-project-wbs';
import { createNextNativeProjectWbsReader } from '../adapters/server/next-native-project-wbs';

export { nativeProjectsAvailable };
export type {
  NativeProjectRow,
  NativeProjectMilestoneRow,
  NativeProjectWbs,
  NativeProjectsPageContext,
} from './native-project-wbs';

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativeProjectsPageContext(actor: WorkContext): NativeProjectsPageContext {
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

function expected(actor: NativeProjectsPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam)
    throw new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeProjectsPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_PROJECTS_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeProjectsClient(nativeProjectsConfiguration(raw));
}

export async function nativeProjectWbs(
  actor: NativeProjectsPageContext,
  holdingSlug: string,
  projectId: string,
): Promise<NativeProjectWbs | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED');
  return createNextNativeProjectWbsReader((incoming, id, input, signal) =>
    native.wbs(incoming, actor, id, input, signal),
  )(projectId, { holdingSlug, ...scope(actor), expected: expected(actor) });
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeProjectsFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeProjectsError)) return undefined;
  if (error.code === 'NATIVE_PROJECTS_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_PROJECTS_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return { status: 404, code: 'NOT_FOUND', message: 'Project not found in this scope.' };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_PROJECTS_UNAVAILABLE',
      message: 'The native Projects service is unavailable.',
    };
  if (error.code === 'NATIVE_PROJECTS_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_PROJECTS_UNAVAILABLE',
      message: 'The native Projects reader is unavailable.',
    };
  if (error.code === 'NATIVE_PROJECTS_REQUEST_INVALID')
    return {
      status: 422,
      code: 'PROJECTS_REQUEST_INVALID',
      message: 'Review the Projects request.',
    };
  if (
    error.code === 'NATIVE_PROJECTS_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_PROJECTS_COOKIE_LIMIT' ||
    error.code === 'NATIVE_PROJECTS_RESPONSE_INVALID' ||
    error.code === 'NATIVE_PROJECTS_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_PROJECTS_UNAVAILABLE',
      message: 'The native Projects reader is unavailable.',
    };
  return undefined;
}
