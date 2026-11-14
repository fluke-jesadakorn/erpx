import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  createNativePeoplePayrollClient,
  nativePeoplePayrollAvailable,
  nativePeoplePayrollConfiguration,
  NativePeoplePayrollError,
  type NativePayrollRun,
  type NativePayrollRunDetail,
  type NativePayrollSummaryRow,
  type NativePayrollWorkRow,
  type NativePeoplePayrollPageContext,
} from './native-payroll-summary';
import {
  createNextNativePeoplePayrollDetailReader,
  createNextNativePeoplePayrollRunsReader,
  createNextNativePeoplePayrollSummaryReader,
  createNextNativePeoplePayrollWorkRowsReader,
} from '../adapters/server/next-native-payroll-summary';

export { nativePeoplePayrollAvailable };
export type {
  NativePayrollRun,
  NativePayrollRunDetail,
  NativePayrollSummaryRow,
  NativePayrollWorkRow,
  NativePeoplePayrollPageContext,
} from './native-payroll-summary';

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function peoplePayrollPageContext(actor: WorkContext): NativePeoplePayrollPageContext {
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

function expected(actor: NativePeoplePayrollPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam)
    throw new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativePeoplePayrollPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG;
  if (raw === undefined) return undefined;
  return createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(raw));
}

export async function nativePeoplePayrollSummary(
  actor: NativePeoplePayrollPageContext,
  holdingSlug: string,
  payMonth?: string,
): Promise<NativePayrollSummaryRow[] | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED');
  return createNextNativePeoplePayrollSummaryReader((incoming, input, signal) =>
    native.summary(incoming, actor, input, signal),
  )({ holdingSlug, ...scope(actor), payMonth, expected: expected(actor) });
}

export async function nativePeoplePayrollRuns(
  actor: NativePeoplePayrollPageContext,
  holdingSlug: string,
): Promise<NativePayrollRun[] | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED');
  return createNextNativePeoplePayrollRunsReader((incoming, input, signal) =>
    native.runs(incoming, actor, input, signal),
  )({ holdingSlug, ...scope(actor), expected: expected(actor) });
}

export async function nativePeoplePayrollDetail(
  actor: NativePeoplePayrollPageContext,
  holdingSlug: string,
  runId: string,
): Promise<NativePayrollRunDetail | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED');
  return createNextNativePeoplePayrollDetailReader((incoming, input, signal) =>
    native.detail(incoming, actor, input, runId, signal),
  )({ holdingSlug, ...scope(actor), expected: expected(actor) });
}

export async function nativePeoplePayrollWorkRows(
  actor: NativePeoplePayrollPageContext,
  holdingSlug: string,
): Promise<NativePayrollWorkRow[] | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED');
  return createNextNativePeoplePayrollWorkRowsReader((incoming, input, signal) =>
    native.workRows(incoming, actor, input, signal),
  )({ holdingSlug, ...scope(actor), expected: expected(actor) });
}


/** Dispatch a payroll run command (create/calculate/cancel) to the native owner. */
export async function nativePeoplePayrollCommand(
  incoming: Pick<Headers, 'get'>,
  actor: NativePeoplePayrollPageContext,
  holdingSlug: string,
  commandPath: string,
  body: unknown,
  key: string,
  origin?: string,
): Promise<unknown | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED');
  return native.command(
    incoming,
    actor,
    { holdingSlug, ...scope(actor), expected: expected(actor) },
    commandPath,
    body,
    key,
    origin ?? `https://${incoming.get('host') ?? ''}`,
  );
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativePeoplePayrollFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativePeoplePayrollError)) return undefined;
  if (error.code === 'NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_PEOPLE_PAYROLL_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return { status: 404, code: 'NOT_FOUND', message: 'Payroll source not found in this scope.' };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_PEOPLE_PAYROLL_UNAVAILABLE',
      message: 'The native People Payroll service is unavailable.',
    };
  if (error.code === 'NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_PEOPLE_PAYROLL_UNAVAILABLE',
      message: 'The native People Payroll reader is unavailable.',
    };
  if (error.code === 'NATIVE_PEOPLE_PAYROLL_REQUEST_INVALID')
    return {
      status: 422,
      code: 'PEOPLE_PAYROLL_REQUEST_INVALID',
      message: 'Review the People Payroll request.',
    };
  if (
    error.code === 'NATIVE_PEOPLE_PAYROLL_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_PEOPLE_PAYROLL_COOKIE_LIMIT' ||
    error.code === 'NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID' ||
    error.code === 'NATIVE_PEOPLE_PAYROLL_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_PEOPLE_PAYROLL_UNAVAILABLE',
      message: 'The native People Payroll reader is unavailable.',
    };
  return undefined;
}
