import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  createNativeMarketingCampaignClient,
  nativeMarketingCampaignAvailable,
  nativeMarketingCampaignConfiguration,
  NativeMarketingCampaignError,
  type NativeMarketingCampaignPageContext,
  type NativeMarketingCampaignReadInput,
  type NativeMarketingCampaignReport,
} from './native-marketing-campaign';
import { createNextNativeMarketingCampaignReader } from '../adapters/server/next-native-marketing-campaign';

export { nativeMarketingCampaignAvailable };
export type {
  NativeMarketingAttribution,
  NativeMarketingCampaign,
  NativeMarketingCampaignPageContext,
  NativeMarketingCampaignReport,
  NativeMarketingPurchaseControls,
} from './native-marketing-campaign';

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativeMarketingCampaignPageContext(
  actor: WorkContext,
): NativeMarketingCampaignPageContext {
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

function expected(actor: NativeMarketingCampaignPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam)
    throw new NativeMarketingCampaignError('NATIVE_MARKETING_CAMPAIGN_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeMarketingCampaignPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_MARKETING_CAMPAIGN_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeMarketingCampaignClient(nativeMarketingCampaignConfiguration(raw));
}

/**
 * Native campaign/attribution report. An unset configuration retains the
 * TypeScript workspace; once configured, an invalid or failed native read is
 * surfaced and never silently replaced.
 */
export async function nativeMarketingCampaignReport(
  actor: WorkContext,
): Promise<NativeMarketingCampaignReport | undefined> {
  const native = client();
  if (!native) return undefined;
  const pageContext = nativeMarketingCampaignPageContext(actor);
  const input: NativeMarketingCampaignReadInput = {
    holdingSlug: actor.holdingSlug,
    ...scope(pageContext),
    expected: expected(pageContext),
  };
  try {
    return await createNextNativeMarketingCampaignReader(
      (incoming, readInput, signal) => native.report(incoming, pageContext, readInput, signal),
    )(input);
  } catch (error) {
    const failure = nativeMarketingCampaignFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeMarketingCampaignFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeMarketingCampaignError)) return undefined;
  if (error.code === 'NATIVE_MARKETING_CAMPAIGN_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_MARKETING_CAMPAIGN_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: 'NOT_FOUND',
      message: 'The Marketing campaign report is not available in this scope.',
    };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_MARKETING_CAMPAIGN_UNAVAILABLE',
      message: 'The native Marketing service is unavailable.',
    };
  if (error.code === 'NATIVE_MARKETING_CAMPAIGN_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_MARKETING_CAMPAIGN_UNAVAILABLE',
      message: 'The native Marketing reader is unavailable.',
    };
  if (error.code === 'NATIVE_MARKETING_CAMPAIGN_REQUEST_INVALID')
    return {
      status: 422,
      code: 'MARKETING_CAMPAIGN_REQUEST_INVALID',
      message: 'Review the Marketing campaign request.',
    };
  if (
    error.code === 'NATIVE_MARKETING_CAMPAIGN_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_MARKETING_CAMPAIGN_COOKIE_LIMIT' ||
    error.code === 'NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID' ||
    error.code === 'NATIVE_MARKETING_CAMPAIGN_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_MARKETING_CAMPAIGN_UNAVAILABLE',
      message: 'The native Marketing reader is unavailable.',
    };
  return undefined;
}
