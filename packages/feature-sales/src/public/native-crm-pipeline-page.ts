import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  createNativeCrmPipelineClient,
  nativeCrmPipelineAvailable,
  nativeCrmPipelineConfiguration,
  NativeCrmPipelineError,
  type NativeCrmPipelinePage,
  type NativeCrmPipelinePageContext,
  type NativeCrmPipelineReadInput,
} from './native-crm-pipeline';
import { createNextNativeCrmPipelineReader } from '../adapters/server/next-native-crm-pipeline';

export { nativeCrmPipelineAvailable };
export type {
  NativeCrmForecastTotal,
  NativeCrmPipelinePage,
  NativeCrmPipelinePageContext,
  NativeCrmPipelineRow,
  NativeCrmPurchaseControls,
} from './native-crm-pipeline';

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativeCrmPipelinePageContext(actor: WorkContext): NativeCrmPipelinePageContext {
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

function expected(actor: NativeCrmPipelinePageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam)
    throw new NativeCrmPipelineError('NATIVE_CRM_PIPELINE_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeCrmPipelinePageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_CRM_PIPELINE_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeCrmPipelineClient(nativeCrmPipelineConfiguration(raw));
}

/**
 * Native pipeline list read. An unset configuration retains the TypeScript
 * workspace; once configured, an invalid or failed native read is surfaced and
 * never silently replaced.
 */
export async function nativeCrmPipeline(
  actor: WorkContext,
  query: { q?: string; stage?: string; cursor?: string },
): Promise<NativeCrmPipelinePage | undefined> {
  const native = client();
  if (!native) return undefined;
  const pageContext = nativeCrmPipelinePageContext(actor);
  const input: NativeCrmPipelineReadInput = {
    holdingSlug: actor.holdingSlug,
    search: query.q,
    stage: query.stage,
    cursor: query.cursor,
    ...scope(pageContext),
    expected: expected(pageContext),
  };
  try {
    return await createNextNativeCrmPipelineReader(
      (incoming, readInput, signal) => native.pipeline(incoming, pageContext, readInput, signal),
    )(input);
  } catch (error) {
    const failure = nativeCrmPipelineFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeCrmPipelineFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeCrmPipelineError)) return undefined;
  if (error.code === 'NATIVE_CRM_PIPELINE_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_CRM_PIPELINE_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: 'NOT_FOUND',
      message: 'The CRM pipeline is not available in this scope.',
    };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_CRM_PIPELINE_UNAVAILABLE',
      message: 'The native CRM service is unavailable.',
    };
  if (error.code === 'NATIVE_CRM_PIPELINE_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_CRM_PIPELINE_UNAVAILABLE',
      message: 'The native CRM reader is unavailable.',
    };
  if (error.code === 'NATIVE_CRM_PIPELINE_REQUEST_INVALID')
    return {
      status: 422,
      code: 'CRM_PIPELINE_REQUEST_INVALID',
      message: 'Review the CRM pipeline request.',
    };
  if (
    error.code === 'NATIVE_CRM_PIPELINE_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_CRM_PIPELINE_COOKIE_LIMIT' ||
    error.code === 'NATIVE_CRM_PIPELINE_RESPONSE_INVALID' ||
    error.code === 'NATIVE_CRM_PIPELINE_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_CRM_PIPELINE_UNAVAILABLE',
      message: 'The native CRM reader is unavailable.',
    };
  return undefined;
}
