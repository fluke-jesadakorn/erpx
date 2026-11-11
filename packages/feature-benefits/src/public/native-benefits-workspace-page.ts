import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import type {
  benefitDetail as retainedBenefitDetail,
  benefitsWorkspace as retainedBenefitsWorkspace,
} from '@erpx/server-benefits/public/benefits';
import {
  createNativeBenefitsClient,
  nativeBenefitsAvailable,
  nativeBenefitsConfiguration,
  NativeBenefitsError,
  type NativeBenefitCommandReceipt,
  type NativeBenefitsPageContext,
  type NativeBenefitsReadInput,
} from './native-benefits-workspace';
import { createNextNativeBenefitsWorkspaceReader } from '../adapters/server/next-native-benefits-workspace';

export { nativeBenefitsAvailable };
export type {
  NativeBenefitCommandReceipt,
  NativeBenefitDependent,
  NativeBenefitElection,
  NativeBenefitPlan,
  NativeBenefitWindow,
  NativeBenefitsPageContext,
  NativeBenefitsPurchaseControls,
  NativeBenefitsWorkspace,
} from './native-benefits-workspace';

/** The native read must retain the exact retained workspace wire shape. */
export type NativeBenefitsWorkspaceResult = Awaited<ReturnType<typeof retainedBenefitsWorkspace>>;

/** The native detail must retain the exact retained detail wire shape. */
export type NativeBenefitDetailResult = Awaited<ReturnType<typeof retainedBenefitDetail>>;

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativeBenefitsPageContext(actor: WorkContext): NativeBenefitsPageContext {
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

function expected(actor: NativeBenefitsPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam)
    throw new NativeBenefitsError('NATIVE_BENEFITS_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeBenefitsPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_BENEFITS_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeBenefitsClient(nativeBenefitsConfiguration(raw));
}

/**
 * Native Benefits enrollment workspace. An unset configuration retains the
 * TypeScript workspace; once configured, an invalid or failed native read is
 * surfaced and never silently replaced.
 */
export async function nativeBenefitsWorkspace(
  actor: WorkContext,
  personal = false,
): Promise<NativeBenefitsWorkspaceResult | undefined> {
  try {
    const native = client();
    if (!native) return undefined;
    const pageContext = nativeBenefitsPageContext(actor);
    const input: NativeBenefitsReadInput = {
      holdingSlug: actor.holdingSlug,
      personal,
      ...scope(pageContext),
      expected: expected(pageContext),
    };
    return await createNextNativeBenefitsWorkspaceReader(
      (incoming, readInput, signal) => native.workspace(incoming, pageContext, readInput, signal),
    )(input);
  } catch (error) {
    const failure = nativeBenefitsFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/**
 * Native Benefits election detail. An unset configuration retains the
 * TypeScript detail; once configured, an invalid or failed native read is
 * surfaced and never silently replaced.
 */
export async function nativeBenefitDetail(
  actor: WorkContext,
  id: string,
): Promise<NativeBenefitDetailResult | undefined> {
  try {
    const native = client();
    if (!native) return undefined;
    const pageContext = nativeBenefitsPageContext(actor);
    const input: NativeBenefitsReadInput = {
      holdingSlug: actor.holdingSlug,
      personal: false,
      ...scope(pageContext),
      expected: expected(pageContext),
    };
    return (await native.detail(
      await headers(),
      pageContext,
      input,
      id,
    )) as NativeBenefitDetailResult;
  } catch (error) {
    const failure = nativeBenefitsFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/** Native command path. Unset configuration retains the TypeScript command;
 * once configured, an invalid or failed native command is surfaced. */
export async function nativeBenefitCommand(
  actor: WorkContext,
  body: unknown,
  key: string,
  incoming: Pick<Headers, 'get'>,
): Promise<NativeBenefitCommandReceipt | undefined> {
  try {
    const native = client();
    if (!native) return undefined;
    return await native.command(incoming, nativeBenefitsPageContext(actor), body, key);
  } catch (error) {
    const failure = nativeBenefitsFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeBenefitsFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeBenefitsError)) return undefined;
  if (error.code === 'NATIVE_BENEFITS_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_BENEFITS_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: 'NOT_FOUND',
      message: 'The Benefits workspace is not available in this scope.',
    };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_BENEFITS_UNAVAILABLE',
      message: 'The native Benefits service is unavailable.',
    };
  if (error.code === 'NATIVE_BENEFITS_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_BENEFITS_UNAVAILABLE',
      message: 'The native Benefits reader is unavailable.',
    };
  if (error.code === 'NATIVE_BENEFITS_REQUEST_INVALID')
    return {
      status: 422,
      code: 'BENEFITS_WORKSPACE_REQUEST_INVALID',
      message: 'Review the Benefits workspace request.',
    };
  if (
    error.code === 'NATIVE_BENEFITS_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_BENEFITS_COOKIE_LIMIT' ||
    error.code === 'NATIVE_BENEFITS_RESPONSE_INVALID' ||
    error.code === 'NATIVE_BENEFITS_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_BENEFITS_UNAVAILABLE',
      message: 'The native Benefits reader is unavailable.',
    };
  return undefined;
}
