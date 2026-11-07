import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import { holdingResources } from '@erpx/server-runtime/public/runtime';
import { customerAccessPurchaseControls, requireCustomerAccessPurchasedRead } from '@erpx/server-customer_authority/public/portal-purchases';
import {
  createNativeCustomerAccessClient,
  nativeCustomerAccessAvailable,
  nativeCustomerAccessConfiguration,
  NativeCustomerAccessError,
  type NativeCustomerAccessDetail,
  type NativeCustomerAccessPageContext,
  type NativeCustomerAccessReadInput,
  type NativeCustomerAccessReceipt,
  type NativeCustomerAccessWorkspace,
} from './native-customer-access';
import { createNextNativeCustomerAccessReader } from '../adapters/server/next-native-customer-access';

export { nativeCustomerAccessAvailable };
export type {
  NativeCustomerAccessCommandInput,
  NativeCustomerAccessDetail,
  NativeCustomerAccessPageContext,
  NativeCustomerAccessReceipt,
  NativeCustomerAccessWorkspace,
} from './native-customer-access';

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativeCustomerAccessPageContext(
  actor: WorkContext,
): NativeCustomerAccessPageContext {
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

function expected(context: NativeCustomerAccessPageContext) {
  if (!context.authority || !context.iam)
    throw new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_CONTEXT_CHANGED');
  return {
    companyId: context.companyId,
    assignmentId: context.assignmentId,
    revision: context.authority.revision,
    assignmentVersion: context.authority.assignmentVersion,
  } as const;
}

function scope(context: NativeCustomerAccessPageContext) {
  return {
    branchSlug: context.scope.kind === 'holding' ? undefined : context.scope.branchSlug,
    departmentSlug:
      context.scope.kind === 'department' ? context.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG;
  if (raw === undefined) return undefined;
  try {
    return createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(raw));
  } catch (error) {
    const failure = nativeCustomerAccessFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

async function purchasePreflight(actor: WorkContext, id?: string) {
  const write = (await holdingResources(actor.holdingId)).write;
  await requireCustomerAccessPurchasedRead(write, actor, id);
  return customerAccessPurchaseControls(write, actor);
}

function readInput(actor: NativeCustomerAccessPageContext): NativeCustomerAccessReadInput {
  return {
    holdingSlug: actor.holdingSlug,
    ...scope(actor),
    expected: expected(actor),
  };
}

/**
 * Native customer access workspace/detail reads. An unset configuration
 * retains the TypeScript reader; once configured, an invalid or failed native
 * read is surfaced and never silently replaced.
 */
export async function nativeCustomerAccessWorkspace(
  actor: WorkContext,
): Promise<NativeCustomerAccessWorkspace | undefined> {
  const native = client();
  if (!native) return undefined;
  const context = nativeCustomerAccessPageContext(actor);
  const input = readInput(context);
  const purchase = await purchasePreflight(actor);
  try {
    const workspace = await createNextNativeCustomerAccessReader()((incoming, signal) =>
      native.workspace(incoming, context, input, signal),
    );
    return (purchase ? { ...workspace, purchase } : workspace) as NativeCustomerAccessWorkspace;
  } catch (error) {
    const failure = nativeCustomerAccessFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

export async function nativeCustomerAccessDetail(
  actor: WorkContext,
  id: string,
): Promise<NativeCustomerAccessDetail | undefined> {
  const native = client();
  if (!native) return undefined;
  const context = nativeCustomerAccessPageContext(actor);
  const input = readInput(context);
  const purchase = await purchasePreflight(actor, id);
  try {
    const detail = await createNextNativeCustomerAccessReader()((incoming, signal) =>
      native.detail(incoming, context, id, input, signal),
    );
    return (purchase ? { ...detail, purchase } : detail) as NativeCustomerAccessDetail;
  } catch (error) {
    const failure = nativeCustomerAccessFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/**
 * Native request/approve/reject/revoke command. Returns undefined when the
 * configuration is unset or the input belongs to another retained Customer
 * Authority action (invitation, portal support). A configured eligible command
 * never falls back to the retained command when the native owner fails.
 */
export async function nativeCustomerAccessCommand(
  actor: WorkContext,
  input: Record<string, unknown>,
  key: string,
): Promise<NativeCustomerAccessReceipt | undefined> {
  const id = typeof input.id === 'string' ? input.id : undefined;
  const action =
    typeof input.action === 'string' &&
    ['approve', 'reject', 'revoke'].includes(input.action)
      ? (input.action as 'approve' | 'reject' | 'revoke')
      : undefined;
  const request = id === undefined && input.action === undefined;
  const change = id !== undefined && action !== undefined;
  if (!request && !change) return undefined;
  const native = client();
  if (!native) return undefined;
  const context = nativeCustomerAccessPageContext(actor);
  const readContext = readInput(context);
  const purchase = await purchasePreflight(actor, change ? id : undefined);
  if (purchase) {
    const allowed = request || action === 'approve' ? purchase.newWork : purchase.settleExisting;
    if (!allowed)
      throw new ApiError(
        403,
        'CUSTOMER_ACCESS_PURCHASE_REQUIRED',
        'Your current Company package does not include this customer access command.',
      );
  }
  try {
    if (id !== undefined && action !== undefined) {
      const body = JSON.stringify({
        action,
        expectedVersion: input.expectedVersion,
        reason: input.reason,
      });
      return await createNextNativeCustomerAccessReader()((incoming, signal) =>
        native.change(incoming, context, id, body, key, readContext, signal),
      );
    }
    const body = JSON.stringify({
      orderId: input.orderId,
      sourceVersion: input.sourceVersion,
      email: input.email,
      expiresAt: input.expiresAt,
      reason: input.reason,
    });
    return await createNextNativeCustomerAccessReader()((incoming, signal) =>
      native.request(incoming, context, body, key, readContext, signal),
    );
  } catch (error) {
    const failure = nativeCustomerAccessFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeCustomerAccessFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeCustomerAccessError)) return undefined;
  if (error.code === 'NATIVE_CUSTOMER_ACCESS_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_CUSTOMER_ACCESS_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: 'NOT_FOUND',
      message: 'The customer access request is not available in this scope.',
    };
  if (error.status === 409) return { status: 409, code: error.code, message: error.message };
  if (error.status === 422) return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_CUSTOMER_ACCESS_UNAVAILABLE',
      message: 'The native customer access service is unavailable.',
    };
  if (error.code === 'NATIVE_CUSTOMER_ACCESS_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_CUSTOMER_ACCESS_UNAVAILABLE',
      message: 'The native customer access reader is unavailable.',
    };
  if (error.code === 'NATIVE_CUSTOMER_ACCESS_REQUEST_INVALID')
    return {
      status: 422,
      code: 'CUSTOMER_ACCESS_REQUEST_INVALID',
      message: 'Review the customer access request.',
    };
  if (
    error.code === 'NATIVE_CUSTOMER_ACCESS_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_CUSTOMER_ACCESS_COOKIE_LIMIT' ||
    error.code === 'NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID' ||
    error.code === 'NATIVE_CUSTOMER_ACCESS_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_CUSTOMER_ACCESS_UNAVAILABLE',
      message: 'The native customer access reader is unavailable.',
    };
  return undefined;
}
