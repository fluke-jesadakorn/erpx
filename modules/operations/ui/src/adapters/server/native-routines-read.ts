import 'server-only';
import { request as httpRequest } from 'node:http';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import type {ScheduleRow} from '@erpx/operations-server/public/routines';
import type {RoutinePurchase,RoutinePurchases} from '@erpx/operations-server/public/rules/routine-purchase-controls';

/** Complete native routine editor context. The Rust owner composes every field;
 *  the Next transport accepts all of it and prepares no owner SQL or authority. */
export type NativeRoutineEditorContext = {
  projects: Array<{id: string; title: string}>;
  options: {
    people: Array<{
      user_id: string;
      legal_name: string;
      department_id: string | null;
      level_id: string | null;
      level_name: string | null;
    }>;
    levels: Array<{id: string; name: string; rank: number}>;
    departments: Array<{id: string; display_name: string}>;
  };
  canWrite: boolean;
  purchases: RoutinePurchases | null;
};

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
const shape = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  record(value) && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));

const purchase = (value: unknown): value is RoutinePurchase =>
  shape(value, ['newWork', 'settleExisting', 'history', 'revision']) &&
  typeof value.newWork === 'boolean' && typeof value.settleExisting === 'boolean' &&
  typeof value.history === 'boolean' && text(value.revision, 2000);
const purchases = (value: unknown): value is RoutinePurchases | null =>
  value === null || (shape(value, ['work', 'projects', 'context']) &&
    purchase(value.work) && purchase(value.projects) && text(value.context, 200));

function editorContext(value: unknown): value is NativeRoutineEditorContext {
  if (!shape(value, ['projects', 'options', 'canWrite', 'purchases'])) return false;
  if (!Array.isArray(value.projects) ||
      !value.projects.every(p => shape(p, ['id', 'title']) && id(p.id) && typeof p.title === 'string')) return false;
  const options = value.options;
  if (!shape(options, ['people', 'levels', 'departments'])) return false;
  if (!Array.isArray(options.people) ||
      !options.people.every(p => shape(p, ['user_id', 'legal_name', 'department_id', 'level_id', 'level_name']) &&
        id(p.user_id) && typeof p.legal_name === 'string' && optionalId(p.department_id) &&
        optionalId(p.level_id) && (p.level_name === null || typeof p.level_name === 'string'))) return false;
  if (!Array.isArray(options.levels) ||
      !options.levels.every(l => shape(l, ['id', 'name', 'rank']) &&
        id(l.id) && typeof l.name === 'string' && Number.isSafeInteger(l.rank))) return false;
  if (!Array.isArray(options.departments) ||
      !options.departments.every(d => shape(d, ['id', 'display_name']) &&
        id(d.id) && typeof d.display_name === 'string')) return false;
  return typeof value.canWrite === 'boolean' && purchases(value.purchases);
}

type RoutineReadTransport = (url: string, init: RequestInit) => Promise<Response>;
type RoutineWorkSelection = {
  companyId: string;
  assignmentId: string;
  authority?: { revision: number; assignmentVersion: number };
  scope: { kind: string; branchSlug?: string; departmentSlug?: string };
};

export function routineReadHeaders(
  input: HeadersInit, actor: RoutineWorkSelection, appsAdmission?: 'purchased' | 'deferred',
): Headers {
  if (!actor.authority)
    throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'Reload your workspace and try again.');
  const headers = new Headers(input);
  headers.delete('x-erpx-routine-apps-admission');
  if (appsAdmission) headers.set('x-erpx-routine-apps-admission', appsAdmission);
  headers.set('x-erpx-context-company', actor.companyId);
  headers.set('x-erpx-context-assignment', actor.assignmentId);
  headers.set('x-erpx-context-revision', String(actor.authority.revision));
  headers.set('x-erpx-context-assignment-version', String(actor.authority.assignmentVersion));
  headers.delete('x-erpx-workspace-branch');
  headers.delete('x-erpx-workspace-department');
  if (actor.scope.branchSlug) headers.set('x-erpx-workspace-branch', actor.scope.branchSlug);
  if (actor.scope.departmentSlug) headers.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  return headers;
}

/** Forward the selected work context and the registered Holding Host verbatim. */
function routineHeaders(request: Request, userId: string): Headers {
  const headers = new Headers({
    host: request.headers.get('host') ?? '',
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  for (const name of [
    'cookie', 'origin', 'x-erpx-context-company', 'x-erpx-context-assignment',
    'x-erpx-context-revision', 'x-erpx-context-assignment-version',
    'x-erpx-workspace-branch', 'x-erpx-workspace-department', 'x-erpx-routine-apps-admission',
  ]) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  headers.set('x-erpx-context-user', userId);
  return headers;
}

/** Node fetch replaces Host with the private loopback address. */
const direct: RoutineReadTransport = (url, init) => new Promise((resolve, reject) => {
  const request = httpRequest(url, {
    method: 'GET', headers: Object.fromEntries(new Headers(init.headers)), signal: init.signal ?? undefined, agent: false,
  }, response => {
    const status = response.statusCode ?? 502;
    const chunks: Uint8Array[] = [];
    let length = 0;
    response.on('data', (chunk: Buffer) => {
      length += chunk.length;
      if (length > 16 * 1024 * 1024) response.destroy(new Error('NATIVE_ROUTINES_RESPONSE_TOO_LARGE'));
      else chunks.push(chunk);
    });
    response.once('error', reject);
    response.once('end', () => {
      const body = [204, 205, 304].includes(status) ? null : Buffer.concat(chunks).toString('utf8');
      resolve(new Response(body, {status}));
    });
  });
  request.once('error', reject);
  request.end();
});

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
  transport: RoutineReadTransport = direct,
): Promise<NativeRoutineTemplateRow[]> {
  const config = configuration();
  if (!new URL(request.url).host) throw invalid();
  if (!id(holdingSlug) || !id(userId)) throw invalid();
  const headers = routineHeaders(request, userId);
  const url = `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/routine-templates`;
  let response: Response;
  try {
    response = await transport(url, {
      method: 'GET', headers, cache: 'no-store',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    throw new ApiError(503, 'NATIVE_ROUTINES_UNAVAILABLE', 'The native Operations routine read is unavailable.');
  }
  const payload: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    throw readError(response.status, payload);
  }
  const data = envelope(payload, headers);
  if (!Array.isArray(data) || data.length > 5000 || !data.every((item) => row(item,
      headers.get('x-erpx-context-company') ?? '')))
    throw invalid();
  return data as NativeRoutineTemplateRow[];
}

/** Read the complete native routine editor context for both SSR callers. The
 *  Rust owner is authoritative for picker options, Projects, write authority
 *  and purchase controls; the response envelope is checked against the same
 *  work context headers. No TypeScript business read or fallback is consulted. */
export async function nativeRoutineEditorContext(
  request: Request,
  holdingSlug: string,
  userId: string,
  transport: RoutineReadTransport = direct,
): Promise<NativeRoutineEditorContext> {
  const config = configuration();
  if (!new URL(request.url).host) throw invalid();
  if (!id(holdingSlug) || !/^[a-z0-9_-]+$/.test(holdingSlug)) throw invalid();
  if (!id(userId)) throw invalid();
  const headers = routineHeaders(request, userId);
  if (process.env.NODE_ENV === 'production') headers.set('x-erpx-routine-compiled-render', '1');
  const url = `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/routine-template-editor-context`;
  let response: Response;
  try {
    response = await transport(url, {
      method: 'GET', headers, cache: 'no-store',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    throw new ApiError(503, 'NATIVE_ROUTINES_UNAVAILABLE', 'The native Operations routine read is unavailable.');
  }
  const payload: unknown = await response.json().catch(() => null);
  if (!response.ok) throw readError(response.status, payload);
  const data = envelope(payload, headers);
  if (!editorContext(data)) throw invalid();
  return data;
}

/**
 * Strict transport for the native routine-template detail read. The Rust owner
 * executes the alias-qualified SELECT and returns the complete retired row;
 * this module only forwards session/context headers and validates the
 * envelope. A missing template is `null`; every other incomplete answer fails
 * closed. The retired TypeScript `getTemplate` path no longer exists, so there
 * is no fallback.
 */
export async function nativeRoutineTemplateDetail(
  request: Request,
  holdingSlug: string,
  templateId: string,
  userId: string,
  transport: RoutineReadTransport = direct,
): Promise<NativeRoutineTemplateRow | null> {
  const {data, companyId} = await readTemplate(request, holdingSlug, templateId, userId, '', transport);
  if (data === null) return null;
  if (!row(data, companyId) || data.id !== templateId) throw invalid();
  return data;
}

export async function nativeRoutineTemplateSchedules(
  request: Request,
  holdingSlug: string,
  templateId: string,
  userId: string,
  transport: RoutineReadTransport = direct,
): Promise<{template: NativeRoutineTemplateRow; schedules: ScheduleRow[]; canEdit: boolean}> {
  const {data, companyId} = await readTemplate(request, holdingSlug, templateId, userId, '/schedules', transport);
  if (!record(data) || !row(data.template, companyId) || data.template.id !== templateId ||
      !Array.isArray(data.schedules) || !data.schedules.every(value => scheduleRow(value, companyId, templateId)) ||
      typeof data.canEdit !== 'boolean') throw invalid();
  return {template: data.template, schedules: data.schedules, canEdit: data.canEdit};
}

function scheduleRow(value: unknown, companyId: string, templateId: string): value is ScheduleRow {
  if (!record(value) || !id(value.id) || value.company_id !== companyId || value.template_id !== templateId ||
      !['daily', 'weekly', 'monthly'].includes(String(value.frequency)) ||
      !['active', 'paused', 'ended'].includes(String(value.state)) ||
      !text(value.timezone, 128) || !text(value.occurrence_time, 5) || !text(value.start_date, 10) ||
      !(value.end_date === null || text(value.end_date, 10)) ||
      !(value.last_error === null || text(value.last_error))) return false;
  const optionalInteger = (number: unknown) => number === null || integer(number);
  return [value.weekday, value.month_day, value.next_occurrence_at, value.paused_at, value.last_generated_at].every(optionalInteger) &&
    [value.due_offset_hours, value.created_at, value.updated_at].every(number => integer(number)) && integer(value.version, 1);
}

async function readTemplate(
  request: Request,
  holdingSlug: string,
  templateId: string,
  userId: string,
  suffix: '' | '/schedules',
  transport: RoutineReadTransport,
) {
  const config = configuration();
  if (!new URL(request.url).host) throw invalid();
  if (!id(holdingSlug) || !/^[a-z0-9_-]+$/.test(holdingSlug)) throw invalid();
  if (!id(templateId) || !/^[A-Za-z0-9_.:-]+$/.test(templateId)) throw invalid();
  if (!id(userId)) throw invalid();
  const headers = routineHeaders(request, userId);
  const url = `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/routine-templates/${encodeURIComponent(templateId)}${suffix}`;
  let response: Response;
  try {
    response = await transport(url, {
      method: 'GET', headers, cache: 'no-store',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    throw new ApiError(503, 'NATIVE_ROUTINES_UNAVAILABLE', 'The native Operations routine read is unavailable.');
  }
  const payload: unknown = await response.json().catch(() => null);
  if (!response.ok) throw readError(response.status, payload);
  return {data: envelope(payload, headers), companyId: headers.get('x-erpx-context-company') ?? ''};
}

function readError(status: number, payload: unknown) {
  const error = record(payload) && record(payload.error) ? payload.error : payload;
  const code = record(error) && text(error.code, 200) ? error.code : 'NATIVE_ROUTINES_FAILED';
  const message = record(error) && text(error.message, 2000) ? error.message : 'The routine read failed.';
  return new ApiError(status, code, message);
}
