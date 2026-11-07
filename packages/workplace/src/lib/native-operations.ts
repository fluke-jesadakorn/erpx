import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  createNativeOperationsClient,
  nativeOperationsConfiguration,
  NativeOperationsError,
  type NativeOperationsPageContext,
  type NativeOperationsReadInput,
  type NativeOperationsReceipt,
} from '@erpx/feature-operations/public/native-operations';
import type {
  OperationsDetail,
  OperationsWorkspace,
} from '@erpx/feature-operations/lib/operations-work-model';

function expected(actor: NativeOperationsPageContext) {
  if (!actor.authority) throw new NativeOperationsError('NATIVE_OPERATIONS_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: actor.authority.revision,
    assignmentVersion: actor.authority.assignmentVersion,
  } as const;
}

function pageContext(actor: WorkContext): NativeOperationsPageContext {
  if (!actor.authority || !actor.iam) throw new NativeOperationsError('NATIVE_OPERATIONS_CONTEXT_CHANGED');
  const scope =
    actor.scope.kind === 'holding'
      ? ({ kind: 'holding' } as const)
      : actor.scope.kind === 'branch'
        ? ({ kind: 'branch', branchSlug: actor.scope.branchSlug } as const)
        : ({
            kind: 'department',
            ...(actor.scope.branchSlug ? { branchSlug: actor.scope.branchSlug } : {}),
            departmentSlug: actor.scope.departmentSlug,
          } as const);
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
    scope,
    authority: {
      revision: actor.authority.revision,
      assignmentVersion: actor.authority.assignmentVersion,
    },
    modules: [...actor.modules],
    iam: { permissions: [...actor.iam.permissions] },
  };
}

function scopeInput(actor: NativeOperationsPageContext) {
  if (actor.scope.kind === 'holding') return {};
  if (actor.scope.kind === 'branch') return { branchSlug: actor.scope.branchSlug };
  return {
    ...(actor.scope.branchSlug ? { branchSlug: actor.scope.branchSlug } : {}),
    departmentSlug: actor.scope.departmentSlug,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_OPERATIONS_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeOperationsClient(nativeOperationsConfiguration(raw));
}

export function nativeOperationsAvailable(): boolean {
  return process.env.ERPX_NATIVE_OPERATIONS_CONFIG !== undefined;
}

export async function nativeOperationsWorkspace(
  actor: WorkContext,
  holdingSlug: string,
): Promise<OperationsWorkspace | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeOperationsError('NATIVE_OPERATIONS_CONTEXT_CHANGED');
  const page = pageContext(actor);
  const input: NativeOperationsReadInput = {
    holdingSlug,
    ...scopeInput(page),
    expected: expected(page),
  };
  return native.workspace(await headers(), page, input);
}

export async function nativeOperationsDetail(
  actor: WorkContext,
  holdingSlug: string,
  id: string,
): Promise<OperationsDetail | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeOperationsError('NATIVE_OPERATIONS_CONTEXT_CHANGED');
  if (!id || id.length > 200) throw new NativeOperationsError('NATIVE_OPERATIONS_REQUEST_INVALID');
  const page = pageContext(actor);
  const input = {
    holdingSlug,
    id,
    ...scopeInput(page),
    expected: expected(page),
  } satisfies NativeOperationsReadInput & { id: string };
  return native.detail(await headers(), page, input);
}

export async function nativeOperationsCommand(
  actor: WorkContext,
  holdingSlug: string,
  body: string,
  key: string,
): Promise<NativeOperationsReceipt | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeOperationsError('NATIVE_OPERATIONS_CONTEXT_CHANGED');
  if (!body || body.length > 64 * 1024 || !key)
    throw new NativeOperationsError('NATIVE_OPERATIONS_REQUEST_INVALID');
  const page = pageContext(actor);
  const input: NativeOperationsReadInput = {
    holdingSlug,
    ...scopeInput(page),
    expected: expected(page),
  };
  return native.command(await headers(), page, input, body, key);
}

export function nativeOperationsFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeOperationsError)) return undefined;
  if (error.code === 'NATIVE_OPERATIONS_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_OPERATIONS_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return { status: 404, code: error.code, message: 'Operations exception not found in this scope.' };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_OPERATIONS_UNAVAILABLE',
      message: 'The native Operations service is unavailable.',
    };
  if (error.code === 'NATIVE_OPERATIONS_REQUEST_INVALID')
    return {
      status: 422,
      code: 'OPERATIONS_INPUT_INVALID',
      message: 'Review the Operations command.',
    };
  if (
    error.code === 'NATIVE_OPERATIONS_CONFIGURATION_INVALID' ||
    error.code === 'NATIVE_OPERATIONS_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_OPERATIONS_COOKIE_LIMIT' ||
    error.code === 'NATIVE_OPERATIONS_RESPONSE_INVALID' ||
    error.code === 'NATIVE_OPERATIONS_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_OPERATIONS_UNAVAILABLE',
      message: 'The native Operations service is unavailable.',
    };
  return undefined;
}
