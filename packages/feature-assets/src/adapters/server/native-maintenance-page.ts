import 'server-only';
import {
  createNativeMaintenanceClient,
  nativeMaintenanceConfiguration,
  NativeMaintenanceError,
  type NativeMaintenancePageContext,
} from './native-maintenance';
import type {
  NativeMaintenanceDetail,
  NativeMaintenancePlan,
  NativeMaintenancePlanDetail,
  NativeMaintenanceWorkspace,
} from '../../lib/maintenance-view';
import {
  createNextNativeMaintenanceCommand,
  createNextNativeMaintenanceDetailReader,
  createNextNativeMaintenancePlanDetailReader,
  createNextNativeMaintenancePlanReader,
  createNextNativeMaintenanceReader,
} from './next-native-maintenance';

function expected(actor: NativeMaintenancePageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam) throw new NativeMaintenanceError('NATIVE_MAINTENANCE_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeMaintenancePageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_MAINTENANCE_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeMaintenanceClient(nativeMaintenanceConfiguration(raw));
}

/** Selector presence only; unset configuration keeps the explicit fail-closed
 * page/route policy without probing the private service. */
export function nativeMaintenanceAvailable(): boolean {
  return process.env.ERPX_NATIVE_MAINTENANCE_CONFIG !== undefined;
}

export async function nativeMaintenanceWorkspace(
  actor: NativeMaintenancePageContext,
  holdingSlug: string,
): Promise<NativeMaintenanceWorkspace | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeMaintenanceError('NATIVE_MAINTENANCE_CONTEXT_CHANGED');
  return createNextNativeMaintenanceReader((incoming, input, signal) =>
    native.workspace(incoming, actor, input, signal),
  )({ holdingSlug, ...scope(actor), expected: expected(actor) });
}

export async function nativeMaintenanceDetail(
  actor: NativeMaintenancePageContext,
  holdingSlug: string,
  id: string,
): Promise<NativeMaintenanceDetail | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeMaintenanceError('NATIVE_MAINTENANCE_CONTEXT_CHANGED');
  if (typeof id !== 'string' || id.length === 0 || id.length > 200)
    throw new NativeMaintenanceError('NATIVE_MAINTENANCE_REQUEST_INVALID');
  return createNextNativeMaintenanceDetailReader((incoming, input, signal) =>
    native.detail(incoming, actor, input, signal),
  )({ holdingSlug, id, ...scope(actor), expected: expected(actor) });
}

export async function nativeMaintenancePlans(
  actor: NativeMaintenancePageContext,
  holdingSlug: string,
): Promise<NativeMaintenancePlan[] | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeMaintenanceError('NATIVE_MAINTENANCE_CONTEXT_CHANGED');
  return createNextNativeMaintenancePlanReader((incoming, input, signal) =>
    native.plans(incoming, actor, input, signal),
  )({ holdingSlug, ...scope(actor), expected: expected(actor) });
}

export async function nativeMaintenancePlanDetail(
  actor: NativeMaintenancePageContext,
  holdingSlug: string,
  id: string,
): Promise<NativeMaintenancePlanDetail | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeMaintenanceError('NATIVE_MAINTENANCE_CONTEXT_CHANGED');
  if (typeof id !== 'string' || id.length === 0 || id.length > 200)
    throw new NativeMaintenanceError('NATIVE_MAINTENANCE_REQUEST_INVALID');
  return createNextNativeMaintenancePlanDetailReader((incoming, input, signal) =>
    native.planDetail(incoming, actor, input, signal),
  )({ holdingSlug, id, ...scope(actor), expected: expected(actor) });
}

export async function nativeMaintenanceCommand(
  actor: NativeMaintenancePageContext,
  holdingSlug: string,
  body: unknown,
  idempotencyKey: string,
): Promise<{ id: string; version: number; status: string } | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeMaintenanceError('NATIVE_MAINTENANCE_CONTEXT_CHANGED');
  return createNextNativeMaintenanceCommand((incoming, input) =>
    native.command(incoming, actor, input),
  )({
    holdingSlug,
    ...scope(actor),
    expected: expected(actor),
    body,
    idempotencyKey,
  });
}

export async function nativeMaintenancePlanCommand(
  incoming: Pick<Headers, 'get'>,
  actor: NativeMaintenancePageContext,
  holdingSlug: string,
  body: unknown,
  idempotencyKey: string,
): Promise<unknown | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeMaintenanceError('NATIVE_MAINTENANCE_CONTEXT_CHANGED');
  return native.planCommand(incoming, actor, {
    holdingSlug,
    ...scope(actor),
    expected: expected(actor),
    body,
    idempotencyKey,
  });
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeMaintenanceFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeMaintenanceError)) return undefined;
  if (error.code === 'NATIVE_MAINTENANCE_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_MAINTENANCE_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return { status: 404, code: 'NOT_FOUND', message: 'Maintenance record not found in this scope.' };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return { status: 503, code: 'NATIVE_MAINTENANCE_UNAVAILABLE', message: 'The native Maintenance service is unavailable.' };
  if (error.code === 'NATIVE_MAINTENANCE_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_MAINTENANCE_UNAVAILABLE',
      message: 'The native Maintenance reader is unavailable.',
    };
  return undefined;
}
