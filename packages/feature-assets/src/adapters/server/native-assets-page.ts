import 'server-only';
import {
  createNativeAssetsClient,
  nativeAssetsConfiguration,
  NativeAssetsError,
  type NativeAssetsPageContext,
} from './native-assets';
import type { NativeAssetDetail, NativeAssetWorkspace } from '../../lib/asset-view';
import {
  createNextNativeAssetsCommand,
  createNextNativeAssetsDetailReader,
  createNextNativeAssetsReader,
} from './next-native-assets';

function expected(actor: NativeAssetsPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam) throw new NativeAssetsError('NATIVE_ASSETS_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeAssetsPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_ASSETS_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeAssetsClient(nativeAssetsConfiguration(raw));
}

/** Selector presence only; unset configuration keeps the explicit fail-closed
 * page/route policy without probing the private service. */
export function nativeAssetsAvailable(): boolean {
  return process.env.ERPX_NATIVE_ASSETS_CONFIG !== undefined;
}

/** Explicit opt-in; an invalid configuration or failed native read never falls
 * back silently to the replaced TypeScript asset reader. */
export async function nativeAssetWorkspace(
  actor: NativeAssetsPageContext,
  holdingSlug: string,
): Promise<NativeAssetWorkspace | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeAssetsError('NATIVE_ASSETS_CONTEXT_CHANGED');
  const read = native.read;
  return createNextNativeAssetsReader((incoming, input, signal) =>
    read(incoming, actor, input, signal),
  )({ holdingSlug, ...scope(actor), expected: expected(actor) });
}

export async function nativeAssetDetail(
  actor: NativeAssetsPageContext,
  holdingSlug: string,
  id: string,
): Promise<NativeAssetDetail | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeAssetsError('NATIVE_ASSETS_CONTEXT_CHANGED');
  if (typeof id !== 'string' || id.length === 0 || id.length > 200)
    throw new NativeAssetsError('NATIVE_ASSETS_REQUEST_INVALID');
  const detail = native.detail;
  return createNextNativeAssetsDetailReader((incoming, input, signal) =>
    detail(incoming, actor, input, signal),
  )({ holdingSlug, id, ...scope(actor), expected: expected(actor) });
}

export async function nativeAssetCommand(
  actor: NativeAssetsPageContext,
  holdingSlug: string,
  body: unknown,
  idempotencyKey: string,
): Promise<{ id: string; version: number; status: string } | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeAssetsError('NATIVE_ASSETS_CONTEXT_CHANGED');
  const command = native.command;
  return createNextNativeAssetsCommand((incoming, input) =>
    command(incoming, actor, input),
  )({
    holdingSlug,
    ...scope(actor),
    expected: expected(actor),
    body,
    idempotencyKey,
  });
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeAssetsFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeAssetsError)) return undefined;
  if (error.code === 'NATIVE_ASSETS_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before submitting.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return {
      status: 403,
      code: error.code === 'NATIVE_ASSETS_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return { status: 404, code: 'NOT_FOUND', message: 'Asset not found in this scope.' };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return { status: 503, code: 'NATIVE_ASSETS_UNAVAILABLE', message: 'The native Assets service is unavailable.' };
  if (error.code === 'NATIVE_ASSETS_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_ASSETS_UNAVAILABLE',
      message: 'The native Assets reader is unavailable.',
    };
  return undefined;
}
