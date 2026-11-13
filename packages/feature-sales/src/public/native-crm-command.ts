import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  directHttp,
  nativeCrmPipelineConfiguration,
  NativeCrmPipelineError,
  type NativeCrmPipelineConfiguration,
  type NativeCrmPipelinePageContext,
} from './native-crm-pipeline';
import { nativeCrmPipelinePageContext } from './native-crm-pipeline-page';

type Row = Record<string, unknown>;

export class NativeCrmCommandError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native CRM command could not be completed.');
    this.name = 'NativeCrmCommandError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeCrmCommandError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;

/** Exactly the owner-complete native slice. Everything else stays retained. */
export const NATIVE_CRM_LEAD_ACTIONS = new Set(['reassign', 'disqualify', 'reopen']);
export const NATIVE_CRM_OPPORTUNITY_ACTIONS = new Set([
  'link-quote',
  'negotiate',
  'lose',
  'reopen',
  'win',
]);

export function nativeCrmCommandAvailable(): boolean {
  return process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG !== undefined;
}

function configuration(): NativeCrmPipelineConfiguration | undefined {
  const raw = process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG;
  if (raw === undefined) return undefined;
  try {
    return nativeCrmPipelineConfiguration(raw);
  } catch (error) {
    if (error instanceof NativeCrmPipelineError)
      return failure('NATIVE_CRM_COMMAND_CONFIGURATION_INVALID');
    throw error;
  }
}

type NativeOperation = Readonly<{
  kind: 'lead' | 'opportunity';
  action: string;
  body: Row;
}>;

function positive(value: unknown, code: string): number {
  if (!integer(value, 1)) return failure(code, 422);
  return value;
}

function requiredText(value: unknown, code: string): string {
  if (!identifier(value)) return failure(code, 422);
  return value;
}

function nativeOperation(input: unknown): NativeOperation | undefined {
  if (!record(input)) return undefined;
  if (input.domain === 'identity') return undefined;
  const id = input.id;
  if (typeof id !== 'string' || id.length === 0) return undefined;
  if (input.domain !== undefined && input.domain !== 'opportunity') return undefined;
  const action = input.action;
  if (typeof action !== 'string') return undefined;
  if (input.domain === 'opportunity') {
    if (!NATIVE_CRM_OPPORTUNITY_ACTIONS.has(action)) return undefined;
    const body: Row = {
      action,
      expectedVersion: positive(input.expectedVersion, 'NATIVE_CRM_COMMAND_REQUEST_INVALID'),
      leadVersion: positive(input.leadVersion, 'NATIVE_CRM_COMMAND_REQUEST_INVALID'),
      reason: requiredText(input.reason, 'NATIVE_CRM_COMMAND_REQUEST_INVALID'),
    };
    if (action === 'link-quote') {
      body.quoteId = requiredText(input.quoteId, 'NATIVE_CRM_COMMAND_REQUEST_INVALID');
      body.quoteVersion = positive(input.quoteVersion, 'NATIVE_CRM_COMMAND_REQUEST_INVALID');
    }
    if (action === 'win') {
      body.salesId = requiredText(input.salesId, 'NATIVE_CRM_COMMAND_REQUEST_INVALID');
      body.salesVersion = positive(input.salesVersion, 'NATIVE_CRM_COMMAND_REQUEST_INVALID');
    }
    if (action === 'lose' && input.lossReason !== undefined) {
      if (
        typeof input.lossReason !== 'string' ||
        !['price', 'availability', 'competitor', 'timing', 'no_response', 'other'].includes(
          input.lossReason,
        )
      )
        return failure('NATIVE_CRM_COMMAND_REQUEST_INVALID', 422);
      body.lossReason = input.lossReason;
    }
    return { kind: 'opportunity', action, body };
  }
  if (!NATIVE_CRM_LEAD_ACTIONS.has(action)) return undefined;
  const body: Row = {
    action,
    expectedVersion: positive(input.expectedVersion, 'NATIVE_CRM_COMMAND_REQUEST_INVALID'),
    reason: requiredText(input.reason, 'NATIVE_CRM_COMMAND_REQUEST_INVALID'),
  };
  if (action === 'reassign') {
    body.ownerId = requiredText(input.ownerId, 'NATIVE_CRM_COMMAND_REQUEST_INVALID');
    const dueDate = input.dueDate;
    if (typeof dueDate !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(dueDate))
      return failure('NATIVE_CRM_COMMAND_REQUEST_INVALID', 422);
    body.dueDate = dueDate;
  }
  return { kind: 'lead', action, body };
}

function scope(actor: NativeCrmPipelinePageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_CRM_COMMAND_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (response.status !== 200) {
    const body = response.body ? new Uint8Array(await response.arrayBuffer()) : new Uint8Array();
    let code: string | undefined;
    let message: string | undefined;
    if (contentType === 'application/json' && body.byteLength <= 64 * 1024) {
      try {
        const value: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body));
        if (record(value) && record(value.error)) {
          if (text(value.error.code, 200)) code = value.error.code;
          if (text(value.error.message, 2000)) message = value.error.message;
        }
      } catch {
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(code ?? 'NATIVE_CRM_COMMAND_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_CRM_COMMAND_RESPONSE_INVALID');
  }
  const bytes = response.body
    ? new Uint8Array(await response.arrayBuffer())
    : new Uint8Array();
  if (bytes.byteLength > 256 * 1024) return failure('NATIVE_CRM_COMMAND_RESPONSE_INVALID');
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  } catch {
    return failure('NATIVE_CRM_COMMAND_RESPONSE_INVALID');
  }
}

function receipt(value: unknown, id: string, operation: NativeOperation): Row {
  if (operation.kind === 'lead') {
    if (
      !exact(value, ['id', 'version', 'opportunityId']) ||
      value.id !== id ||
      !integer(value.version, 1) ||
      value.opportunityId !== null
    )
      return failure('NATIVE_CRM_COMMAND_RESPONSE_INVALID');
    return value;
  }
  const stage =
    operation.action === 'link-quote'
      ? 'proposal'
      : operation.action === 'negotiate'
        ? 'negotiation'
        : operation.action === 'lose'
          ? 'lost'
          : operation.action === 'win'
            ? 'won'
            : 'discovery';
  if (
    !exact(value, ['id', 'version', 'leadVersion', 'stage']) ||
    value.id !== id ||
    !integer(value.version, 1) ||
    !integer(value.leadVersion, 1) ||
    value.stage !== stage
  )
    return failure('NATIVE_CRM_COMMAND_RESPONSE_INVALID');
  return value;
}

/**
 * Send one owner-complete native CRM command. Returns `undefined` when the
 * command is not part of the native slice or configuration is absent, so the
 * caller keeps the retained TypeScript path. A configured native failure never
 * falls back.
 */
export async function nativeCrmCommand(
  actor: WorkContext,
  holdingSlug: string,
  input: unknown,
  key: string,
  incoming: Pick<Headers, 'get'>,
  transport: typeof directHttp = directHttp,
): Promise<Row | undefined> {
  const config = configuration();
  if (!config) return undefined;
  const operation = nativeOperation(input);
  if (!operation) return undefined;
  const id = (input as Row).id as string;
  const pageContext = nativeCrmPipelinePageContext(actor);
  if (!pageContext.authority || !pageContext.iam)
    return failure('NATIVE_CRM_COMMAND_CONTEXT_CHANGED');
  const host = incoming.get('host');
  const holding = host
    ? config.holdings.find(
        (value) =>
          new URL(value.origin).host === host &&
          value.holdingSlug === holdingSlug &&
          value.holdingId === actor.holdingId,
      )
    : undefined;
  if (!holding) return failure('NATIVE_CRM_COMMAND_AUTHORITY_INVALID');
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'content-type': 'application/json',
    'idempotency-key': key,
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return failure('NATIVE_CRM_COMMAND_COOKIE_LIMIT');
    headers.set('cookie', cookie);
  }
  for (const [name, slug] of [
    ['x-erpx-workspace-branch', scope(pageContext).branchSlug],
    ['x-erpx-workspace-department', scope(pageContext).departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return failure('NATIVE_CRM_COMMAND_REQUEST_INVALID', 422);
      headers.set(name, slug);
    }
  }
  headers.set('x-erpx-context-company', actor.companyId);
  headers.set('x-erpx-context-assignment', pageContext.assignmentId);
  headers.set('x-erpx-context-revision', String(pageContext.authority.revision));
  headers.set(
    'x-erpx-context-assignment-version',
    String(pageContext.authority.assignmentVersion),
  );
  const path =
    operation.kind === 'lead'
      ? `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/crm/leads/${encodeURIComponent(id)}/changes`
      : `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/crm/leads/${encodeURIComponent(id)}/opportunity/changes`;
  const url = new URL(path, config.privateBaseUrl);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await transport(
      'POST',
      url,
      headers,
      JSON.stringify(operation.body),
      controller.signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeCrmCommandError) throw error;
      return failure('NATIVE_CRM_COMMAND_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    return receipt(value, id, operation);
  } finally {
    clearTimeout(timer);
  }
}

/** Bounded failure policy for the retained route error handling. */
export function nativeCrmCommandFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeCrmCommandError)) return undefined;
  if (error.code === 'NATIVE_CRM_COMMAND_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return { status: 403, code: error.code, message: error.message };
  if (error.status === 404)
    return {
      status: 404,
      code: 'NOT_FOUND',
      message: 'The CRM record is not available in this scope.',
    };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_CRM_COMMAND_UNAVAILABLE',
      message: 'The native CRM command service is unavailable.',
    };
  if (error.code === 'NATIVE_CRM_COMMAND_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_CRM_COMMAND_UNAVAILABLE',
      message: 'The native CRM command service is unavailable.',
    };
  if (
    error.code === 'NATIVE_CRM_COMMAND_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_CRM_COMMAND_COOKIE_LIMIT' ||
    error.code === 'NATIVE_CRM_COMMAND_RESPONSE_INVALID' ||
    error.code === 'NATIVE_CRM_COMMAND_UNAVAILABLE'
  )
    return {
      status: 503,
      code: 'NATIVE_CRM_COMMAND_UNAVAILABLE',
      message: 'The native CRM command service is unavailable.',
    };
  return undefined;
}

/** Route-facing wrapper that maps native failures to the retained policy. */
export async function nativeCrmCommandOrThrow(
  request: Request,
  actor: WorkContext,
  holdingSlug: string,
  input: unknown,
  key: string,
): Promise<Row | undefined> {
  try {
    return await nativeCrmCommand(actor, holdingSlug, input, key, request.headers);
  } catch (error) {
    const failurePolicy = nativeCrmCommandFailure(error);
    if (failurePolicy)
      throw new ApiError(failurePolicy.status, failurePolicy.code, failurePolicy.message);
    throw error;
  }
}

/** Exactly the native CRM activity command slice. */
export const NATIVE_CRM_ACTIVITY_ACTIONS = new Set(['complete', 'cancel', 'reassign']);

export function nativeCrmActivityCommandAvailable(): boolean {
  return process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG !== undefined;
}

const activityTypes = new Set(['call', 'email', 'meeting', 'task']);
const isoDate = (value: unknown): value is string =>
  typeof value === 'string' &&
  /^\d{4}-\d{2}-\d{2}$/.test(value) &&
  !Number.isNaN(Date.parse(`${value}T00:00:00Z`));
const activityText = (value: unknown, min: number, max: number): value is string =>
  typeof value === 'string' &&
  value.trim().length >= min &&
  new TextEncoder().encode(value).length <= max;

type NativeActivityOperation =
  | Readonly<{ kind: 'create'; body: Row }>
  | Readonly<{ kind: 'change'; id: string; action: string; body: Row }>;

function nativeActivityOperation(input: unknown): NativeActivityOperation {
  if (!record(input)) return failure('NATIVE_CRM_COMMAND_REQUEST_INVALID', 422);
  if (typeof input.id === 'string') {
    const action = input.action;
    if (typeof action !== 'string' || !NATIVE_CRM_ACTIVITY_ACTIONS.has(action))
      return failure('NATIVE_CRM_COMMAND_REQUEST_INVALID', 422);
    if (!activityText(input.reason, 3, 8192))
      return failure('NATIVE_CRM_COMMAND_REQUEST_INVALID', 422);
    const body: Row = {
      action,
      expectedVersion: positive(input.expectedVersion, 'NATIVE_CRM_COMMAND_REQUEST_INVALID'),
      reason: input.reason,
    };
    if (action === 'reassign') {
      body.ownerId = requiredText(input.ownerId, 'NATIVE_CRM_COMMAND_REQUEST_INVALID');
      if (!isoDate(input.dueDate))
        return failure('NATIVE_CRM_COMMAND_REQUEST_INVALID', 422);
      body.dueDate = input.dueDate;
    }
    return { kind: 'change', id: identifier(input.id) ? input.id : failure('NATIVE_CRM_COMMAND_REQUEST_INVALID', 422), action, body };
  }
  if (input.activityType === undefined || !activityTypes.has(String(input.activityType)))
    return failure('NATIVE_CRM_COMMAND_REQUEST_INVALID', 422);
  if (!identifier(input.leadId) || !identifier(input.ownerId))
    return failure('NATIVE_CRM_COMMAND_REQUEST_INVALID', 422);
  if (
    !activityText(input.subject, 3, 1024) ||
    !activityText(input.details, 3, 8192) ||
    !isoDate(input.dueDate)
  )
    return failure('NATIVE_CRM_COMMAND_REQUEST_INVALID', 422);
  return {
    kind: 'create',
    body: {
      leadId: input.leadId,
      expectedVersion: positive(input.expectedVersion, 'NATIVE_CRM_COMMAND_REQUEST_INVALID'),
      activityType: input.activityType,
      subject: input.subject,
      details: input.details,
      ownerId: input.ownerId,
      dueDate: input.dueDate,
    },
  };
}

function activityContext(value: unknown, actor: WorkContext): Row {
  if (!record(value)) return failure('NATIVE_CRM_COMMAND_RESPONSE_INVALID');
  if (
    value.userId !== actor.userId ||
    value.companyId !== actor.companyId ||
    value.holdingId !== actor.holdingId
  )
    return failure('NATIVE_CRM_COMMAND_CONTEXT_CHANGED');
  return value;
}

function activityReceipt(
  value: unknown,
  id: string,
  operation: NativeActivityOperation,
): Row {
  if (
    !exact(value, ['id', 'version', 'status']) ||
    value.id !== id ||
    !integer(value.version, 1) ||
    typeof value.status !== 'string' ||
    !['open', 'completed', 'cancelled'].includes(value.status)
  )
    return failure('NATIVE_CRM_COMMAND_RESPONSE_INVALID');
  if (operation.kind === 'create' && (value.status !== 'open' || value.version !== 1))
    return failure('NATIVE_CRM_COMMAND_RESPONSE_INVALID');
  if (operation.kind === 'change') {
    const expected =
      operation.action === 'complete'
        ? 'completed'
        : operation.action === 'cancel'
          ? 'cancelled'
          : 'open';
    if (value.status !== expected) return failure('NATIVE_CRM_COMMAND_RESPONSE_INVALID');
  }
  return value;
}

/**
 * Send one native CRM activity create/complete/cancel/reassign command. This
 * slice is native-only: an absent or invalid configuration fails closed and
 * never returns to the retired TypeScript writer.
 */
export async function nativeCrmActivityCommand(
  actor: WorkContext,
  holdingSlug: string,
  input: unknown,
  key: string,
  incoming: Pick<Headers, 'get'>,
  transport: typeof directHttp = directHttp,
): Promise<Row> {
  const config = configuration();
  if (!config) return failure('NATIVE_CRM_COMMAND_UNAVAILABLE', 503);
  const operation = nativeActivityOperation(input);
  const pageContext = nativeCrmPipelinePageContext(actor);
  if (!pageContext.authority || !pageContext.iam)
    return failure('NATIVE_CRM_COMMAND_CONTEXT_CHANGED');
  const host = incoming.get('host');
  const holding = host
    ? config.holdings.find(
        (value) =>
          new URL(value.origin).host === host &&
          value.holdingSlug === holdingSlug &&
          value.holdingId === actor.holdingId,
      )
    : undefined;
  if (!holding) return failure('NATIVE_CRM_COMMAND_AUTHORITY_INVALID');
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'content-type': 'application/json',
    'idempotency-key': key,
    origin: new URL(holding.origin).origin,
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return failure('NATIVE_CRM_COMMAND_COOKIE_LIMIT');
    headers.set('cookie', cookie);
  }
  for (const [name, slug] of [
    ['x-erpx-workspace-branch', scope(pageContext).branchSlug],
    ['x-erpx-workspace-department', scope(pageContext).departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return failure('NATIVE_CRM_COMMAND_REQUEST_INVALID', 422);
      headers.set(name, slug);
    }
  }
  headers.set('x-erpx-context-company', actor.companyId);
  headers.set('x-erpx-context-assignment', pageContext.assignmentId);
  headers.set('x-erpx-context-revision', String(pageContext.authority.revision));
  headers.set(
    'x-erpx-context-assignment-version',
    String(pageContext.authority.assignmentVersion),
  );
  const suffix = encodeURIComponent(holdingSlug);
  const path =
    operation.kind === 'create'
      ? `/api/v1/holdings/${suffix}/crm/activities`
      : `/api/v1/holdings/${suffix}/crm/activities/${encodeURIComponent(operation.id)}/changes`;
  const id =
    operation.kind === 'create'
      ? `crm-activity-${key}`
      : operation.id;
  const url = new URL(path, config.privateBaseUrl);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await transport(
      'POST',
      url,
      headers,
      JSON.stringify(operation.body),
      controller.signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeCrmCommandError) throw error;
      return failure('NATIVE_CRM_COMMAND_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_CRM_COMMAND_RESPONSE_INVALID');
    activityContext(value.context, actor);
    return activityReceipt(value.data, id, operation);
  } finally {
    clearTimeout(timer);
  }
}

/** Route-facing wrapper that maps native activity failures to ApiError. */
export async function nativeCrmActivityCommandOrThrow(
  request: Request,
  actor: WorkContext,
  holdingSlug: string,
  input: unknown,
  key: string,
): Promise<Row> {
  try {
    return await nativeCrmActivityCommand(actor, holdingSlug, input, key, request.headers);
  } catch (error) {
    const failurePolicy = nativeCrmCommandFailure(error);
    if (failurePolicy)
      throw new ApiError(failurePolicy.status, failurePolicy.code, failurePolicy.message);
    throw error;
  }
}
