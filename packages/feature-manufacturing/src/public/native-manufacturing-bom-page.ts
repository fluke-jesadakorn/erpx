import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  createNativeManufacturingBomClient,
  nativeManufacturingAvailable,
  nativeManufacturingConfiguration,
  NativeManufacturingError,
  type NativeManufacturingPageContext,
} from './native-manufacturing-bom';
import type { NativeManufacturingBom } from '../lib/native-manufacturing-types';
import { createNextNativeManufacturingBomReader } from '../adapters/server/next-native-manufacturing-bom';

export { nativeManufacturingAvailable };
export type {
  NativeManufacturingBom,
  NativeManufacturingBomLine,
  NativeManufacturingBomOperation,
  NativeManufacturingBomSpecification,
} from '../lib/native-manufacturing-types';
export type { NativeManufacturingPageContext } from './native-manufacturing-bom';

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativeManufacturingBomPageContext(actor: WorkContext): NativeManufacturingPageContext {
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

function expected(actor: NativeManufacturingPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam)
    throw new NativeManufacturingError('NATIVE_MANUFACTURING_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeManufacturingPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_MANUFACTURING_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeManufacturingBomClient(nativeManufacturingConfiguration(raw));
}

export async function nativeManufacturingBom(
  actor: NativeManufacturingPageContext,
  holdingSlug: string,
): Promise<NativeManufacturingBom | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeManufacturingError('NATIVE_MANUFACTURING_CONTEXT_CHANGED');
  return createNextNativeManufacturingBomReader((incoming, input, signal) =>
    native.bom(incoming, actor, input, signal),
  )({ holdingSlug, ...scope(actor), expected: expected(actor) });
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeManufacturingFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeManufacturingError)) return undefined;
  if (error.code === 'NATIVE_MANUFACTURING_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_MANUFACTURING_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return { status: 404, code: 'NOT_FOUND', message: 'Manufacturing source not found in this scope.' };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_MANUFACTURING_UNAVAILABLE',
      message: 'The native Manufacturing service is unavailable.',
    };
  if (error.code === 'NATIVE_MANUFACTURING_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_MANUFACTURING_UNAVAILABLE',
      message: 'The native Manufacturing reader is unavailable.',
    };
  if (error.code === 'NATIVE_MANUFACTURING_REQUEST_INVALID')
    return {
      status: 422,
      code: 'MANUFACTURING_REQUEST_INVALID',
      message: 'Review the Manufacturing request.',
    };
  if (
    error.code === 'NATIVE_MANUFACTURING_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_MANUFACTURING_COOKIE_LIMIT' ||
    error.code === 'NATIVE_MANUFACTURING_RESPONSE_INVALID' ||
    error.code === 'NATIVE_MANUFACTURING_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_MANUFACTURING_UNAVAILABLE',
      message: 'The native Manufacturing reader is unavailable.',
    };
  return undefined;
}
