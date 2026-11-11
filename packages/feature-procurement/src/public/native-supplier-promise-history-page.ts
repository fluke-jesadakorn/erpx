import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  createNativeSupplierPromiseHistoryClient,
  nativeProcurementAvailable,
  nativeProcurementConfiguration,
  NativeProcurementError,
  type NativeProcurementPageContext,
  type NativeSupplierPromiseDetail,
  type NativeSupplierPromiseReceipt,
} from './native-supplier-promise-history';
import type { NativeSupplierPromiseHistory } from '../lib/native-supplier-promise-history-types';
import {
  createNextNativeSupplierPromiseCommandWriter,
  createNextNativeSupplierPromiseHistoryReader,
} from '../adapters/server/next-native-supplier-promise-history';

export { nativeProcurementAvailable };
export type {
  NativeSupplierPromiseHistory,
  NativeSupplierPromiseHistoryRow,
  NativeSupplierPromiseLineRow,
  NativeOpenPromiseRow,
} from '../lib/native-supplier-promise-history-types';
export type {
  NativeProcurementPageContext,
  NativeSupplierPromiseDetail,
  NativeSupplierPromiseReceipt,
} from './native-supplier-promise-history';

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativeSupplierPromiseHistoryPageContext(actor: WorkContext): NativeProcurementPageContext {
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

function expected(actor: NativeProcurementPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam)
    throw new NativeProcurementError('NATIVE_PROCUREMENT_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeProcurementPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_PROCUREMENT_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(raw));
}

export async function nativeSupplierPromiseDetail(
  actor: NativeProcurementPageContext,
  holdingSlug: string,
  orderId: string,
): Promise<NativeSupplierPromiseDetail | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeProcurementError('NATIVE_PROCUREMENT_CONTEXT_CHANGED');
  return createNextNativeSupplierPromiseHistoryReader((incoming, id, input, signal) =>
    native.detail(incoming, actor, id, input, signal),
  )(orderId, { holdingSlug, ...scope(actor), expected: expected(actor) });
}

export async function recordNativeSupplierPromise(
  actor: NativeProcurementPageContext,
  holdingSlug: string,
  orderId: string,
  key: string,
  body: unknown,
): Promise<NativeSupplierPromiseReceipt | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeProcurementError('NATIVE_PROCUREMENT_CONTEXT_CHANGED');
  return createNextNativeSupplierPromiseCommandWriter(
    (incoming, id, input, commandKey, commandBody, signal) =>
      native.record(incoming, actor, id, input, commandKey, commandBody, signal),
  )(orderId, { holdingSlug, ...scope(actor), expected: expected(actor) }, key, body);
}

export async function nativeSupplierPromiseHistory(
  actor: NativeProcurementPageContext,
  holdingSlug: string,
  orderId: string,
): Promise<NativeSupplierPromiseHistory | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeProcurementError('NATIVE_PROCUREMENT_CONTEXT_CHANGED');
  return createNextNativeSupplierPromiseHistoryReader((incoming, id, input, signal) =>
    native.history(incoming, actor, id, input, signal),
  )(orderId, { holdingSlug, ...scope(actor), expected: expected(actor) });
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeProcurementFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeProcurementError)) return undefined;
  if (error.code === 'NATIVE_PROCUREMENT_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_PROCUREMENT_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return { status: 404, code: 'NOT_FOUND', message: 'Purchase order not found in this scope.' };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_PROCUREMENT_UNAVAILABLE',
      message: 'The native Procurement service is unavailable.',
    };
  if (error.code === 'NATIVE_PROCUREMENT_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_PROCUREMENT_UNAVAILABLE',
      message: 'The native Procurement reader is unavailable.',
    };
  if (error.code === 'NATIVE_PROCUREMENT_REQUEST_INVALID')
    return {
      status: 422,
      code: 'PROCUREMENT_REQUEST_INVALID',
      message: 'Review the Procurement request.',
    };
  if (
    error.code === 'NATIVE_PROCUREMENT_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_PROCUREMENT_COOKIE_LIMIT' ||
    error.code === 'NATIVE_PROCUREMENT_RESPONSE_INVALID' ||
    error.code === 'NATIVE_PROCUREMENT_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_PROCUREMENT_UNAVAILABLE',
      message: 'The native Procurement reader is unavailable.',
    };
  return undefined;
}
