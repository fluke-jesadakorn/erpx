import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import { talentPurchaseControls } from '@erpx/server-talent/public/purchases';
import {
  createNativeTalentClient,
  nativeTalentAvailable,
  nativeTalentConfiguration,
  NativeTalentError,
  type NativeTalentPageContext,
  type NativeTalentReadInput,
  type NativeTalentReceipt,
  type NativeTalentWorkspace,
} from './native-talent-requisition';
import { createNextNativeTalentReader } from '../adapters/server/next-native-talent';

export { nativeTalentAvailable };
export type {
  NativeTalentDetail,
  NativeTalentPageContext,
  NativeTalentReceipt,
  NativeTalentWorkspace,
} from './native-talent-requisition';

const changeActions = new Set(['approve', 'reject', 'open', 'close', 'cancel']);
const settleActions = new Set(['reject', 'close', 'cancel']);

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativeTalentPageContext(actor: WorkContext): NativeTalentPageContext {
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

function expected(context: NativeTalentPageContext) {
  if (!context.authority || !context.iam)
    throw new NativeTalentError('NATIVE_TALENT_CONTEXT_CHANGED');
  return {
    companyId: context.companyId,
    assignmentId: context.assignmentId,
    revision: context.authority.revision,
    assignmentVersion: context.authority.assignmentVersion,
  } as const;
}

function scope(context: NativeTalentPageContext) {
  return {
    branchSlug: context.scope.kind === 'holding' ? undefined : context.scope.branchSlug,
    departmentSlug:
      context.scope.kind === 'department' ? context.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_TALENT_CONFIG;
  if (raw === undefined) return undefined;
  try {
    return createNativeTalentClient(nativeTalentConfiguration(raw));
  } catch (error) {
    const failure = nativeTalentFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

function readInput(actor: NativeTalentPageContext): NativeTalentReadInput {
  return {
    holdingSlug: actor.holdingSlug,
    ...scope(actor),
    expected: expected(actor),
  };
}

const text = (value: unknown, minimum: number, maximum: number): value is string =>
  typeof value === 'string' &&
  [...value].length >= minimum &&
  new TextEncoder().encode(value).length <= maximum;
const date = (value: unknown): value is string => {
  if (typeof value !== 'string') return false;
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return false;
  const year = Number(match[1]),
    month = Number(match[2]),
    day = Number(match[3]);
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  return month >= 1 && month <= 12 && day >= 1 && day <= days[month - 1]!;
};

function createBody(input: Record<string, unknown>): string {
  if (
    !text(input.jobTitle, 2, 160) ||
    !Number.isInteger(input.openings) ||
    Number(input.openings) < 1 ||
    Number(input.openings) > 1_000 ||
    !date(input.targetStartDate) ||
    !text(input.description, 3, 3_000)
  )
    throw new ApiError(
      422,
      'TALENT_REQUISITION_INVALID',
      'Review the requisition details and try again.',
    );
  return JSON.stringify({
    jobTitle: input.jobTitle,
    openings: input.openings,
    targetStartDate: input.targetStartDate,
    description: input.description,
  });
}

function changeBody(input: Record<string, unknown>): string {
  if (
    typeof input.action !== 'string' ||
    !changeActions.has(input.action) ||
    !Number.isInteger(input.expectedVersion) ||
    Number(input.expectedVersion) < 1 ||
    !text(input.reason, 3, 3_000)
  )
    throw new ApiError(
      422,
      'TALENT_REQUISITION_INVALID',
      'Review the requisition change and try again.',
    );
  return JSON.stringify({
    action: input.action,
    expectedVersion: input.expectedVersion,
    reason: input.reason,
  });
}

async function purchasePreflight(
  actor: WorkContext,
  action: string | undefined,
): Promise<void> {
  const purchases = await talentPurchaseControls(actor);
  if (!purchases) return;
  const capability = purchases['recruit.positions'];
  const allowed =
    action === undefined
      ? capability?.newWork
      : settleActions.has(action)
        ? capability?.settleExisting
        : capability?.newWork;
  if (!allowed)
    throw new ApiError(
      403,
      'TALENT_PURCHASE_REQUIRED',
      'Your current Company package does not include this Recruitment requisition command.',
    );
}

/**
 * Native requisition workspace rows. An unset configuration retains the
 * TypeScript reader; once configured, an invalid or failed native read is
 * surfaced and never silently replaced.
 */
export async function nativeTalentWorkspace(
  actor: WorkContext,
): Promise<NativeTalentWorkspace | undefined> {
  const native = client();
  if (!native) return undefined;
  const context = nativeTalentPageContext(actor);
  const input = readInput(context);
  try {
    return await createNextNativeTalentReader()((incoming, signal) =>
      native.workspace(incoming, context, input, signal),
    );
  } catch (error) {
    const failure = nativeTalentFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/**
 * Native requisition create/change command. Returns undefined when the
 * configuration is unset or the input belongs to another retained Talent
 * action (candidate, application, proposal, offer, privacy). A configured
 * eligible command never falls back to the retained command on failure.
 */
export async function nativeTalentRequisitionCommand(
  actor: WorkContext,
  input: Record<string, unknown>,
  key: string,
): Promise<NativeTalentReceipt | undefined> {
  if (input.entity !== 'requisition') return undefined;
  const native = client();
  if (!native) return undefined;
  const context = nativeTalentPageContext(actor);
  const readContext = readInput(context);
  const id = typeof input.id === 'string' ? input.id : undefined;
  const action = typeof input.action === 'string' ? input.action : undefined;
  if (id === undefined && action !== undefined)
    throw new ApiError(
      422,
      'TALENT_REQUISITION_INVALID',
      'A new requisition does not accept a transition action.',
    );
  if (id !== undefined && (action === undefined || !changeActions.has(action)))
    throw new ApiError(
      422,
      'TALENT_REQUISITION_INVALID',
      'Choose a registered requisition transition.',
    );
  await purchasePreflight(actor, id === undefined ? undefined : action);
  try {
    if (id === undefined) {
      const body = createBody(input);
      return await createNextNativeTalentReader()((incoming, signal) =>
        native.create(incoming, context, body, key, readContext, signal),
      );
    }
    const body = changeBody(input);
    return await createNextNativeTalentReader()((incoming, signal) =>
      native.change(incoming, context, id, body, key, readContext, signal),
    );
  } catch (error) {
    if (error instanceof ApiError) throw error;
    const failure = nativeTalentFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeTalentFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeTalentError)) return undefined;
  if (error.code === 'NATIVE_TALENT_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_TALENT_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: 'NOT_FOUND',
      message: 'The requisition is not available in this scope.',
    };
  if (error.status === 409) return { status: 409, code: error.code, message: error.message };
  if (error.status === 422) return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_TALENT_UNAVAILABLE',
      message: 'The native Talent service is unavailable.',
    };
  if (error.code === 'NATIVE_TALENT_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_TALENT_UNAVAILABLE',
      message: 'The native Talent reader is unavailable.',
    };
  if (error.code === 'NATIVE_TALENT_REQUEST_INVALID')
    return {
      status: 422,
      code: 'TALENT_REQUISITION_INVALID',
      message: 'Review the requisition request.',
    };
  if (
    error.code === 'NATIVE_TALENT_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_TALENT_COOKIE_LIMIT' ||
    error.code === 'NATIVE_TALENT_RESPONSE_INVALID' ||
    error.code === 'NATIVE_TALENT_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_TALENT_UNAVAILABLE',
      message: 'The native Talent reader is unavailable.',
    };
  return undefined;
}
