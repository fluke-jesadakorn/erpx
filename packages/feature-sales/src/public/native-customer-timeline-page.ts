import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  createNativeCustomerTimelineClient,
  nativeCustomerTimelineAvailable,
  nativeCustomerTimelineConfiguration,
  NativeCustomerTimelineError,
  type NativeCustomerTimelinePage,
  type NativeCustomerTimelinePageContext,
  type NativeCustomerTimelineReadInput,
} from './native-customer-timeline';
import { createNextNativeCustomerTimelineReader } from '../adapters/server/next-native-customer-timeline';

export { nativeCustomerTimelineAvailable };
export type {
  NativeCustomerTimelineCustomer,
  NativeCustomerTimelineItem,
  NativeCustomerTimelinePage,
  NativeCustomerTimelinePageContext,
} from './native-customer-timeline';

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativeCustomerTimelinePageContext(
  actor: WorkContext,
): NativeCustomerTimelinePageContext {
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

function expected(actor: NativeCustomerTimelinePageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam)
    throw new NativeCustomerTimelineError('NATIVE_CUSTOMER_TIMELINE_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeCustomerTimelinePageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_CUSTOMER_TIMELINE_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'NATIVE_CUSTOMER_TIMELINE_CONFIGURATION_REQUIRED',
      'The native customer timeline is not configured.');
  return createNativeCustomerTimelineClient(nativeCustomerTimelineConfiguration(raw));
}

/** Native customer timeline read. An unset configuration is unavailable; an
 * invalid or failed native read is surfaced and never silently replaced. */
export async function nativeCustomerTimeline(
  actor: WorkContext,
  customerId: string,
  cursor?: string,
  limit?: number,
): Promise<NativeCustomerTimelinePage> {
  const native = client();
  const pageContext = nativeCustomerTimelinePageContext(actor);
  const input: NativeCustomerTimelineReadInput = {
    holdingSlug: actor.holdingSlug,
    customerId,
    cursor,
    limit,
    ...scope(pageContext),
    expected: expected(pageContext),
  };
  try {
    return await createNextNativeCustomerTimelineReader(
      (incoming, readInput, signal) =>
        native.timeline(incoming, pageContext, readInput, signal),
    )(input);
  } catch (error) {
    const failure = nativeCustomerTimelineFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeCustomerTimelineFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeCustomerTimelineError)) return undefined;
  if (error.code === 'NATIVE_CUSTOMER_TIMELINE_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_CUSTOMER_TIMELINE_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: 'NOT_FOUND',
      message: 'The customer timeline is not available in this scope.',
    };
  if (error.status === 409) return { status: 409, code: error.code, message: error.message };
  if (error.status === 422) return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_CUSTOMER_TIMELINE_UNAVAILABLE',
      message: 'The native customer timeline service is unavailable.',
    };
  if (error.code === 'NATIVE_CUSTOMER_TIMELINE_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_CUSTOMER_TIMELINE_UNAVAILABLE',
      message: 'The native customer timeline reader is unavailable.',
    };
  if (error.code === 'NATIVE_CUSTOMER_TIMELINE_REQUEST_INVALID')
    return {
      status: 422,
      code: 'CUSTOMER_TIMELINE_REQUEST_INVALID',
      message: 'Review the customer timeline request.',
    };
  if (
    error.code === 'NATIVE_CUSTOMER_TIMELINE_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_CUSTOMER_TIMELINE_COOKIE_LIMIT' ||
    error.code === 'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID' ||
    error.code === 'NATIVE_CUSTOMER_TIMELINE_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_CUSTOMER_TIMELINE_UNAVAILABLE',
      message: 'The native customer timeline reader is unavailable.',
    };
  return undefined;
}
