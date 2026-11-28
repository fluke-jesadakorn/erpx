import 'server-only';
import {
  createNativeBrandIdentityClient,
  nativeBrandIdentityConfiguration,
  NativeBrandIdentityError,
  type NativeBrandIdentityPageContext,
  type NativeBrandIdentityWorkspace,
} from './native-brand-identity';
import {
  createNextNativeBrandIdentityCommand,
  createNextNativeBrandIdentityReader,
} from './next-native-brand-identity';

function expected(actor: NativeBrandIdentityPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam) throw new NativeBrandIdentityError('NATIVE_BRAND_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeBrandIdentityPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_BRAND_IDENTITY_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeBrandIdentityClient(nativeBrandIdentityConfiguration(raw));
}

/** Explicit opt-in; an invalid configuration or failed native read never falls
 * back silently to the replaced TypeScript writer. */
export async function nativeBrandIdentityWorkspace(
  actor: NativeBrandIdentityPageContext,
  holdingSlug: string,
): Promise<NativeBrandIdentityWorkspace | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeBrandIdentityError('NATIVE_BRAND_CONTEXT_CHANGED');
  const read = native.read;
  return createNextNativeBrandIdentityReader((incoming, input, signal) =>
    read(incoming, actor, input, signal),
  )({ holdingSlug, ...scope(actor), expected: expected(actor) });
}

export async function nativeBrandIdentityCommand(
  actor: NativeBrandIdentityPageContext,
  holdingSlug: string,
  body: unknown,
  idempotencyKey: string,
): Promise<Record<string, unknown> | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeBrandIdentityError('NATIVE_BRAND_CONTEXT_CHANGED');
  const command = native.command;
  return createNextNativeBrandIdentityCommand((incoming, input) =>
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
export function nativeBrandIdentityFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeBrandIdentityError)) return undefined;
  if (error.code === 'NATIVE_BRAND_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before submitting.',
    };
  if (error.code !== 'NATIVE_BRAND_UNAVAILABLE') return undefined;
  switch (error.status) {
    case 401:
      return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
    case 403:
      return { status: 403, code: 'FORBIDDEN', message: 'This action is unavailable in your current role.' };
    case 404:
      return { status: 404, code: 'NOT_FOUND', message: 'Brand identity not found.' };
    case 409:
      return { status: 409, code: 'NATIVE_BRAND_CONFLICT', message: 'The Brand identity changed. Refresh and review again.' };
    default:
      return undefined;
  }
}
