import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  createNativeEcommerceCatalogClient,
  nativeEcommerceCatalogAvailable,
  nativeEcommerceCatalogConfiguration,
  NativeEcommerceCatalogError,
  type NativeEcommerceCatalog,
  type NativeEcommerceCatalogPageContext,
  type NativeEcommerceCatalogReadInput,
} from './native-ecommerce-catalog';
import { createNextNativeEcommerceCatalogReader } from '../adapters/server/next-native-ecommerce-catalog';

export { nativeEcommerceCatalogAvailable };
export type {
  NativeEcommerceCatalog,
  NativeEcommerceCatalogPageContext,
  NativeEcommerceCatalogRow,
} from './native-ecommerce-catalog';

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativeEcommerceCatalogPageContext(actor: WorkContext): NativeEcommerceCatalogPageContext {
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

function expected(actor: NativeEcommerceCatalogPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam)
    throw new NativeEcommerceCatalogError('NATIVE_ECOMMERCE_CATALOG_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeEcommerceCatalogPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_ECOMMERCE_CATALOG_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeEcommerceCatalogClient(nativeEcommerceCatalogConfiguration(raw));
}

export async function nativeEcommerceCatalog(
  actor: WorkContext,
): Promise<NativeEcommerceCatalog | undefined> {
  const native = client();
  if (!native) return undefined;
  const pageContext = nativeEcommerceCatalogPageContext(actor);
  const input: NativeEcommerceCatalogReadInput = {
    holdingSlug: actor.holdingSlug,
    ...scope(pageContext),
    expected: expected(pageContext),
  };
  try {
    return await createNextNativeEcommerceCatalogReader(
      (incoming, readInput, signal) => native.catalog(incoming, pageContext, readInput, signal),
    )(input);
  } catch (error) {
    const failure = nativeEcommerceCatalogFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeEcommerceCatalogFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeEcommerceCatalogError)) return undefined;
  if (error.code === 'NATIVE_ECOMMERCE_CATALOG_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_ECOMMERCE_CATALOG_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404) return { status: 404, code: 'NOT_FOUND', message: 'Not found.' };
  if (error.status === 409)
    return {
      status: 409,
      code: error.code,
      message: error.message,
    };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  return {
    status: 503,
    code: 'NATIVE_ECOMMERCE_CATALOG_UNAVAILABLE',
    message: 'The native Ecommerce catalog is temporarily unavailable.',
  };
}
