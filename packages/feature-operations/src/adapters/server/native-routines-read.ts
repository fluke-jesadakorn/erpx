import 'server-only';
import {ApiError} from '@erpx/server-runtime/public/http';

/**
 * Strict transport for the native routine-template list read. The Rust owner
 * executes the SELECT and returns the complete retired row shape; this module
 * only forwards session/context headers and validates the envelope. There is
 * no fallback to the retired TypeScript `listTemplates` writer path.
 */
export type NativeRoutineTemplateRow = {
  id: string;
  title: string;
  description: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  creator_user_id: string;
  assignee_user_id: string;
  project_id: string | null;
  start_approvers_json: string;
  completion_approvers_json: string;
  published: number;
  version: number;
  created_at: number;
  updated_at: number;
};

type Config = Readonly<{privateBaseUrl: string; timeoutMs: number}>;
const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const rowKeys = [
  'id', 'title', 'description', 'company_id', 'branch_id', 'department_id',
  'creator_user_id', 'assignee_user_id', 'project_id', 'start_approvers_json',
  'completion_approvers_json', 'published', 'version', 'created_at', 'updated_at',
] as const;

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const id = (value: unknown): value is string => text(value, 200) && value.length > 0;
const optionalId = (value: unknown): value is string | null => value === null || id(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;

function configuration(): Config {
  const raw = process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'NATIVE_ROUTINES_UNAVAILABLE', 'The native Operations routine read is unavailable.');
  let value: Config;
  try {
    value = JSON.parse(raw) as Config;
  } catch {
    throw new ApiError(503, 'NATIVE_ROUTINES_CONFIGURATION_INVALID', 'Routine transport is misconfigured.');
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    throw new ApiError(503, 'NATIVE_ROUTINES_CONFIGURATION_INVALID', 'Routine transport is misconfigured.');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username || base.password ||
    value.privateBaseUrl !== base.origin ||
    !integer(value.timeoutMs, 250) || value.timeoutMs > 30_000
  )
    throw new ApiError(503, 'NATIVE_ROUTINES_CONFIGURATION_INVALID', 'Routine transport is misconfigured.');
  return value;
}

export function nativeRoutineTemplatesAvailable(): boolean {
  return process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG !== undefined;
}

function row(value: unknown, companyId: string): value is NativeRoutineTemplateRow {
  if (!record(value) || Object.keys(value).length !== rowKeys.length) return false;
  if (!rowKeys.every((key) => Object.hasOwn(value, key))) return false;
  if (!id(value.id) || !text(value.title, 400) || !text(value.description, 2_097_152)) return false;
  if (value.company_id !== companyId) return false;
  if (!optionalId(value.branch_id) || !optionalId(value.department_id) || !optionalId(value.project_id)) return false;
  if (!id(value.creator_user_id) || !id(value.assignee_user_id)) return false;
  if (!text(value.start_approvers_json, 65_536) || !text(value.completion_approvers_json, 65_536)) return false;
  return integer(value.published) && integer(value.version, 1) &&
    integer(value.created_at) && integer(value.updated_at);
}

function envelope(value: unknown, headers: Headers) {
  if (!record(value) || Object.keys(value).length !== 2 ||
      !Object.hasOwn(value, 'data') || !Object.hasOwn(value, 'context') ||
      !record(value.context) ||
      value.context.companyId !== headers.get('x-erpx-context-company') ||
      value.context.userId !== headers.get('x-erpx-context-user') ||
      value.context.assignmentId !== headers.get('x-erpx-context-assignment') ||
      value.context.revision !== Number(headers.get('x-erpx-context-revision')) ||
      value.context.assignmentVersion !== Number(headers.get('x-erpx-context-assignment-version')))
    throw invalid();
  return value.data;
}

const invalid = () => new ApiError(
  503, 'NATIVE_ROUTINES_RESPONSE_INVALID', 'The native Operations routine read response is invalid.',
);

/** Forward one read with the selected work context; no owner SQL is prepared
 *  here and no retained TypeScript reader is consulted. */
export async function nativeRoutineTemplateList(
  request: Request,
  holdingSlug: string,
  userId: string,
): Promise<NativeRoutineTemplateRow[]> {
  const config = configuration();
  if (!new URL(request.url).host) throw invalid();
  if (!id(holdingSlug) || !id(userId)) throw invalid();
  const headers = new Headers({
    host: request.headers.get('host') ?? '',
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  for (const name of [
    'cookie', 'origin', 'x-erpx-context-company', 'x-erpx-context-assignment',
    'x-erpx-context-revision', 'x-erpx-context-assignment-version',
    'x-erpx-workspace-branch', 'x-erpx-workspace-department',
  ]) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  headers.set('x-erpx-context-user', userId);
  const url = `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/routine-templates`;
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET', headers, cache: 'no-store',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    throw new ApiError(503, 'NATIVE_ROUTINES_UNAVAILABLE', 'The native Operations routine read is unavailable.');
  }
  const payload: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const code = record(payload) && text(payload.code, 200) ? payload.code : 'NATIVE_ROUTINES_FAILED';
    const message = record(payload) && text(payload.message, 2000)
      ? payload.message : 'The routine read failed.';
    throw new ApiError(response.status, code, message);
  }
  const data = envelope(payload, headers);
  if (!Array.isArray(data) || data.length > 5000 || !data.every((item) => row(item,
      headers.get('x-erpx-context-company') ?? '')))
    throw invalid();
  return data as NativeRoutineTemplateRow[];
}

/**
 * Strict transport for the native routine-template detail read. The Rust owner
 * executes the alias-qualified SELECT and returns the complete retired row;
 * this module only forwards session/context headers and validates the
 * envelope. A missing template is `null`; every other incomplete answer fails
 * closed. There is no fallback to the retired TypeScript `getTemplate` path.
 */
export async function nativeRoutineTemplateDetail(
  request: Request,
  holdingSlug: string,
  templateId: string,
  userId: string,
): Promise<NativeRoutineTemplateRow | null> {
  const config = configuration();
  if (!new URL(request.url).host) throw invalid();
  if (!id(holdingSlug) || !/^[a-z0-9_-]+$/.test(holdingSlug)) throw invalid();
  if (!id(templateId) || !/^[A-Za-z0-9_.:-]+$/.test(templateId)) throw invalid();
  if (!id(userId)) throw invalid();
  const headers = new Headers({
    host: request.headers.get('host') ?? '',
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  for (const name of [
    'cookie', 'origin', 'x-erpx-context-company', 'x-erpx-context-assignment',
    'x-erpx-context-revision', 'x-erpx-context-assignment-version',
    'x-erpx-workspace-branch', 'x-erpx-workspace-department',
  ]) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  headers.set('x-erpx-context-user', userId);
  const url = `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/routine-templates/${encodeURIComponent(templateId)}`;
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET', headers, cache: 'no-store',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    throw new ApiError(503, 'NATIVE_ROUTINES_UNAVAILABLE', 'The native Operations routine read is unavailable.');
  }
  const payload: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const code = record(payload) && text(payload.code, 200) ? payload.code : 'NATIVE_ROUTINES_FAILED';
    const message = record(payload) && text(payload.message, 2000)
      ? payload.message : 'The routine read failed.';
    throw new ApiError(response.status, code, message);
  }
  const data = envelope(payload, headers);
  if (data === null) return null;
  if (!row(data, headers.get('x-erpx-context-company') ?? '')) throw invalid();
  return data as NativeRoutineTemplateRow;
}
