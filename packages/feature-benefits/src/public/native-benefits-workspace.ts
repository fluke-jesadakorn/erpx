import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from retained page authorization, never a grant. */
export type NativeBenefitsPageContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  assignmentScope: string;
  scope:
    | { kind: 'holding' }
    | { kind: 'branch'; branchSlug: string }
    | { kind: 'department'; branchSlug?: string; departmentSlug: string };
  authority?: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam?: { permissions: readonly string[] };
};

export type NativeBenefitsConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeBenefitsReadInput = Readonly<{
  holdingSlug: string;
  personal: boolean;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeBenefitPlan = Readonly<{
  id: string;
  company_id: string;
  name: string;
  provider: string;
  coverage_type: 'employee' | 'family';
  currency: string;
  employee_deduction_minor: number;
  employer_cost_minor: number;
  effective_from: string;
  effective_to: string | null;
  status: 'draft' | 'published' | 'rejected' | 'retired';
  version: number;
  created_by: string;
  reviewed_by: string | null;
  created_at: number;
  updated_at: number;
  required_eligibility_code: string | null;
}>;

export type NativeBenefitWindow = Readonly<{
  id: string;
  company_id: string;
  name: string;
  opens_on: string;
  closes_on: string;
  effective_on: string;
  status: 'draft' | 'open' | 'closed' | 'cancelled';
  version: number;
  created_by: string;
  reviewed_by: string | null;
  created_at: number;
  updated_at: number;
}>;

export type NativeBenefitDependent = Readonly<{
  id: string;
  company_id: string;
  person_id: string;
  name: string;
  relationship: string;
  birth_date: string;
  status: 'pending' | 'verified' | 'rejected' | 'removed';
  version: number;
  created_by: string;
  reviewed_by: string | null;
  created_at: number;
  updated_at: number;
}>;

export type NativeBenefitElection = Readonly<{
  id: string;
  company_id: string;
  person_id: string;
  engagement_id: string;
  plan_id: string;
  plan_name: string;
  employee_name: string;
  plan_version: number;
  window_id: string;
  window_version: number;
  coverage_type: 'employee' | 'family';
  currency: string;
  employee_deduction_minor: number;
  employer_cost_minor: number;
  effective_from: string;
  effective_to: string | null;
  status: 'elected' | 'submitted' | 'acknowledged' | 'released' | 'cancelled';
  version: number;
  created_at: number;
  updated_at: number;
}>;

export type NativeBenefitsPurchaseControls = Readonly<{
  newWork: boolean;
  settleExisting: boolean;
  history: boolean;
  revision: string;
}>;

export type NativeBenefitsWorkspace = Readonly<{
  actorId: string;
  canWrite: boolean;
  canReview: boolean;
  canManageDocuments: boolean;
  workContext: string;
  plans: NativeBenefitPlan[];
  windows: NativeBenefitWindow[];
  dependents: NativeBenefitDependent[];
  elections: NativeBenefitElection[];
  purchase?: NativeBenefitsPurchaseControls;
}>;

export type NativeBenefitCommandReceipt = Readonly<{
  id: string;
  status: string;
  version: number;
}>;

export type NativeBenefitsTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeBenefitsError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Benefits request could not be completed.');
    this.name = 'NativeBenefitsError';
    this.code = code;
    this.status = status;
  }
}

const PLAN_LIMIT = 500;
const WINDOW_LIMIT = 500;
const DEPENDENT_LIMIT = 200;
const ELECTION_LIMIT = 1_000;
const NAME_LIMIT = 160;
const PROVIDER_LIMIT = 160;
const RELATIONSHIP_LIMIT = 80;
const ELIGIBILITY_LIMIT = 120;
const WORK_CONTEXT_LIMIT = 64;

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeBenefitsError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' &&
  value.length <= max &&
  !/[\u0000-\u001f\u007f]/.test(value) &&
  value.trim() === value;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0;
const name = (value: unknown, max = NAME_LIMIT): value is string =>
  text(value, max) && [...value].length >= 2;
const code = (value: unknown, max = ELIGIBILITY_LIMIT): value is string =>
  typeof value === 'string' && value.length >= 2 && value.length <= max &&
  value.trim() === value && !value.includes('\u0000');
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const currency = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Z]{3}$/.test(value);
const nullable = <T>(value: unknown, check: (value: unknown) => value is T): value is T | null =>
  value === null || check(value);
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};

const validDate = (value: string): boolean => {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const year = Number(value.slice(0, 4));
  const month = Number(value.slice(5, 7));
  const day = Number(value.slice(8, 10));
  if (year < 1900 || year > 9999 || month < 1 || month > 12) return false;
  const leap = (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
  const days = month === 2 ? (leap ? 29 : 28) : [4, 6, 9, 11].includes(month) ? 30 : 31;
  return day >= 1 && day <= days;
};
const date = (value: unknown): value is string => text(value, 10) && validDate(value);
const effectiveTo = (value: unknown, from: string): value is string | null =>
  value === null || (date(value) && value >= from);

const compareBytes = (left: string, right: string): number => {
  const a = new TextEncoder().encode(left);
  const b = new TextEncoder().encode(right);
  const length = Math.min(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    if (a[index]! !== b[index]!) return a[index]! - b[index]!;
  }
  return a.length - b.length;
};

function ordered<T extends { id: string; updated_at: number }>(rows: T[]): boolean {
  for (let index = 1; index < rows.length; index += 1) {
    const previous = rows[index - 1]!;
    const current = rows[index]!;
    if (
      current.updated_at > previous.updated_at ||
      (current.updated_at === previous.updated_at && compareBytes(previous.id, current.id) >= 0)
    )
      return false;
  }
  return true;
}

function unique<T extends { id: string }>(rows: T[]): boolean {
  const ids = new Set(rows.map((row) => row.id));
  return ids.size === rows.length;
}

const plan = (value: unknown, ctx: NativeBenefitsPageContext): value is NativeBenefitPlan => {
  if (
    !exact(value, [
      'id', 'company_id', 'name', 'provider', 'coverage_type', 'currency',
      'employee_deduction_minor', 'employer_cost_minor', 'effective_from', 'effective_to',
      'status', 'version', 'created_by', 'reviewed_by', 'created_at', 'updated_at',
      'required_eligibility_code',
    ]) ||
    !identifier(value.id) ||
    value.company_id !== ctx.companyId ||
    !name(value.name) ||
    !name(value.provider, PROVIDER_LIMIT) ||
    (value.coverage_type !== 'employee' && value.coverage_type !== 'family') ||
    !currency(value.currency) ||
    !integer(value.employee_deduction_minor) ||
    !integer(value.employer_cost_minor) ||
    !date(value.effective_from) ||
    !effectiveTo(value.effective_to, value.effective_from) ||
    !['draft', 'published', 'rejected', 'retired'].includes(String(value.status)) ||
    !integer(value.version, 1) ||
    !identifier(value.created_by) ||
    !nullable(value.reviewed_by, identifier) ||
    !integer(value.created_at) ||
    !integer(value.updated_at) ||
    !nullable(value.required_eligibility_code, code)
  )
    return false;
  return true;
};

const window = (value: unknown, ctx: NativeBenefitsPageContext): value is NativeBenefitWindow => {
  if (
    !exact(value, [
      'id', 'company_id', 'name', 'opens_on', 'closes_on', 'effective_on', 'status',
      'version', 'created_by', 'reviewed_by', 'created_at', 'updated_at',
    ]) ||
    !identifier(value.id) ||
    value.company_id !== ctx.companyId ||
    !name(value.name) ||
    !date(value.opens_on) ||
    !date(value.closes_on) ||
    value.opens_on > value.closes_on ||
    !date(value.effective_on) ||
    !['draft', 'open', 'closed', 'cancelled'].includes(String(value.status)) ||
    !integer(value.version, 1) ||
    !identifier(value.created_by) ||
    !nullable(value.reviewed_by, identifier) ||
    !integer(value.created_at) ||
    !integer(value.updated_at)
  )
    return false;
  return true;
};

const dependent = (
  value: unknown,
  ctx: NativeBenefitsPageContext,
): value is NativeBenefitDependent => {
  if (
    !exact(value, [
      'id', 'company_id', 'person_id', 'name', 'relationship', 'birth_date', 'status',
      'version', 'created_by', 'reviewed_by', 'created_at', 'updated_at',
    ]) ||
    !identifier(value.id) ||
    value.company_id !== ctx.companyId ||
    !identifier(value.person_id) ||
    !name(value.name) ||
    !name(value.relationship, RELATIONSHIP_LIMIT) ||
    !date(value.birth_date) ||
    !['pending', 'verified', 'rejected', 'removed'].includes(String(value.status)) ||
    !integer(value.version, 1) ||
    !identifier(value.created_by) ||
    !nullable(value.reviewed_by, identifier) ||
    !integer(value.created_at) ||
    !integer(value.updated_at)
  )
    return false;
  return true;
};

const election = (
  value: unknown,
  ctx: NativeBenefitsPageContext,
): value is NativeBenefitElection => {
  if (
    !exact(value, [
      'id', 'company_id', 'person_id', 'engagement_id', 'plan_id', 'plan_name',
      'employee_name', 'plan_version', 'window_id', 'window_version', 'coverage_type',
      'currency', 'employee_deduction_minor', 'employer_cost_minor', 'effective_from',
      'effective_to', 'status', 'version', 'created_at', 'updated_at',
    ]) ||
    !identifier(value.id) ||
    value.company_id !== ctx.companyId ||
    !identifier(value.person_id) ||
    !identifier(value.engagement_id) ||
    !identifier(value.plan_id) ||
    !name(value.plan_name) ||
    !name(value.employee_name) ||
    !integer(value.plan_version, 1) ||
    !identifier(value.window_id) ||
    !integer(value.window_version, 1) ||
    (value.coverage_type !== 'employee' && value.coverage_type !== 'family') ||
    !currency(value.currency) ||
    !integer(value.employee_deduction_minor) ||
    !integer(value.employer_cost_minor) ||
    !date(value.effective_from) ||
    !effectiveTo(value.effective_to, value.effective_from) ||
    !['elected', 'submitted', 'acknowledged', 'released', 'cancelled'].includes(
      String(value.status),
    ) ||
    !integer(value.version, 1) ||
    !integer(value.created_at) ||
    !integer(value.updated_at)
  )
    return false;
  return true;
};

function purchase(value: unknown): NativeBenefitsPurchaseControls | null {
  if (value === null) return null;
  if (
    !exact(value, ['newWork', 'settleExisting', 'history', 'revision']) ||
    typeof value.newWork !== 'boolean' ||
    typeof value.settleExisting !== 'boolean' ||
    typeof value.history !== 'boolean' ||
    !text(value.revision, 200) ||
    value.revision.length === 0
  )
    return failure('NATIVE_BENEFITS_RESPONSE_INVALID');
  return {
    newWork: value.newWork,
    settleExisting: value.settleExisting,
    history: value.history,
    revision: value.revision,
  };
}

function pageData(value: unknown, ctx: NativeBenefitsPageContext, context: Context) {
  if (
    !exact(value, [
      'actorId', 'canWrite', 'canReview', 'canManageDocuments', 'plans', 'windows',
      'dependents', 'elections', 'purchase', 'workContext',
    ]) ||
    value.actorId !== ctx.userId ||
    value.canWrite !== (ctx.iam?.permissions ?? []).includes('records.write') ||
    value.canReview !== (ctx.iam?.permissions ?? []).includes('benefits.review') ||
    value.canManageDocuments !==
      (ctx.iam?.permissions ?? []).includes('benefits.documents.manage') ||
    !Array.isArray(value.plans) ||
    !Array.isArray(value.windows) ||
    !Array.isArray(value.dependents) ||
    !Array.isArray(value.elections) ||
    typeof value.workContext !== 'string' ||
    value.workContext.length !== WORK_CONTEXT_LIMIT ||
    !/^[a-f0-9]+$/.test(value.workContext)
  )
    return failure('NATIVE_BENEFITS_RESPONSE_INVALID');
  if (
    value.plans.length > PLAN_LIMIT ||
    value.windows.length > WINDOW_LIMIT ||
    value.dependents.length > DEPENDENT_LIMIT ||
    value.elections.length > ELECTION_LIMIT
  )
    return failure('NATIVE_BENEFITS_RESPONSE_INVALID');
  if (
    !value.plans.every((item) => plan(item, ctx)) ||
    !value.windows.every((item) => window(item, ctx)) ||
    !value.dependents.every((item) => dependent(item, ctx)) ||
    !value.elections.every((item) => election(item, ctx)) ||
    !ordered(value.plans as NativeBenefitPlan[]) ||
    !ordered(value.windows as NativeBenefitWindow[]) ||
    !ordered(value.dependents as NativeBenefitDependent[]) ||
    !ordered(value.elections as NativeBenefitElection[]) ||
    !unique(value.plans as NativeBenefitPlan[]) ||
    !unique(value.windows as NativeBenefitWindow[]) ||
    !unique(value.dependents as NativeBenefitDependent[]) ||
    !unique(value.elections as NativeBenefitElection[])
  ) {
    return failure('NATIVE_BENEFITS_RESPONSE_INVALID');
  }
  const controls = purchase(value.purchase);
  return {
    actorId: ctx.userId,
    canWrite: value.canWrite as boolean,
    canReview: value.canReview as boolean,
    canManageDocuments: value.canManageDocuments as boolean,
    workContext: value.workContext as string,
    plans: value.plans as NativeBenefitPlan[],
    windows: value.windows as NativeBenefitWindow[],
    dependents: value.dependents as NativeBenefitDependent[],
    elections: value.elections as NativeBenefitElection[],
    ...(controls ? { purchase: controls } : {}),
  } satisfies NativeBenefitsWorkspace;
}

function context(
  value: unknown,
  actor: NativeBenefitsPageContext,
  input: NativeBenefitsReadInput,
): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_BENEFITS_RESPONSE_INVALID');
  const parsed = value as Context;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.branchSlug !== (input.branchSlug ?? null) ||
    parsed.departmentSlug !== (input.departmentSlug ?? null) ||
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== actor.authority?.assignmentVersion ||
    parsed.revision !== actor.authority?.revision ||
    parsed.role !== actor.role ||
    parsed.assignmentScopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    !texts(parsed.modules) ||
    !sameSet(actor.modules, parsed.modules) ||
    !texts(parsed.permissions) ||
    !sameSet(actor.iam?.permissions ?? [], parsed.permissions)
  )
    return failure('NATIVE_BENEFITS_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_BENEFITS_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeBenefitsConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_BENEFITS_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30000 ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_BENEFITS_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string; origin: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_BENEFITS_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_BENEFITS_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
      origin: holding.origin,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeBenefitsTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string; origin: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativeBenefitsReadInput,
  actor: NativeBenefitsPageContext,
  timeout: number,
  signal?: AbortSignal,
  command?: Readonly<{ body: string; key: string; origin: string }>,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeBenefitsError('NATIVE_BENEFITS_AUTHORITY_INVALID'));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeBenefitsError('NATIVE_BENEFITS_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeBenefitsError('NATIVE_BENEFITS_REQUEST_INVALID'));
      headers.set(key, slug);
    }
  }
  if (input.personal) headers.set('x-erpx-benefits-personal', '1');
  if (input.expected) {
    if (
      !identifier(input.expected.companyId) ||
      !identifier(input.expected.assignmentId) ||
      !integer(input.expected.revision) ||
      !integer(input.expected.assignmentVersion, 1)
    )
      return Promise.reject(new NativeBenefitsError('NATIVE_BENEFITS_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  if (command) {
    if (
      command.origin !== holding.origin ||
      incoming.get('origin') !== holding.origin ||
      new TextEncoder().encode(command.body).length > 64 * 1024 ||
      !/^[A-Za-z0-9:_.-]{8,180}$/.test(command.key)
    )
      return Promise.reject(new NativeBenefitsError('NATIVE_BENEFITS_REQUEST_INVALID'));
    headers.set('origin', command.origin);
    headers.set('content-type', 'application/json');
    headers.set('idempotency-key', command.key);
  }
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/benefits`, base);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
  return transport(
    command ? 'POST' : 'GET',
    url,
    headers,
    command?.body,
    controller.signal,
  ).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

async function boundedBytes(response: Response, limit: number): Promise<Uint8Array> {
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > limit) {
        await reader.cancel();
        return failure('NATIVE_BENEFITS_RESPONSE_LIMIT');
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.length;
  }
  return data;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_BENEFITS_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (response.status !== 200) {
    const body = await boundedBytes(response, 64 * 1024);
    let code: string | undefined;
    let message: string | undefined;
    if (contentType === 'application/json') {
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
    return failure(code ?? 'NATIVE_BENEFITS_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_BENEFITS_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_BENEFITS_RESPONSE_INVALID');
  }
}

export function nativeBenefitsAvailable(): boolean {
  return process.env.ERPX_NATIVE_BENEFITS_CONFIG !== undefined;
}

export function nativeBenefitsConfiguration(raw: string): NativeBenefitsConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeBenefitsError('NATIVE_BENEFITS_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_BENEFITS_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_BENEFITS_CONFIGURATION_INVALID');
  return value as unknown as NativeBenefitsConfiguration;
}

function commandReceipt(value: unknown): NativeBenefitCommandReceipt {
  if (
    !exact(value, ['id', 'status', 'version']) ||
    !identifier(value.id) ||
    !text(value.status, 40) ||
    ![
      'draft',
      'published',
      'rejected',
      'retired',
      'open',
      'closed',
      'cancelled',
      'pending',
      'verified',
      'removed',
      'elected',
      'submitted',
      'acknowledged',
      'released',
    ].includes(value.status as string) ||
    !integer(value.version, 1)
  )
    return failure('NATIVE_BENEFITS_RESPONSE_INVALID');
  return {
    id: value.id as string,
    status: value.status as string,
    version: value.version as number,
  };
}

export function createNativeBenefitsClient(
  raw: NativeBenefitsConfiguration,
  transport: NativeBenefitsTransport = directHttp,
) {
  const config = configuration(raw);
  const workspace = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeBenefitsPageContext,
    input: NativeBenefitsReadInput,
    signal?: AbortSignal,
  ): Promise<NativeBenefitsWorkspace> => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_BENEFITS_CONTEXT_CHANGED');
    const response = await transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      input,
      actor,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeBenefitsError) throw error;
      return failure('NATIVE_BENEFITS_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_BENEFITS_RESPONSE_INVALID');
    const parsedContext = context(value.context, actor, input);
    return pageData(value.data, actor, parsedContext);
  };
  const command = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeBenefitsPageContext,
    body: unknown,
    key: string,
  ): Promise<NativeBenefitCommandReceipt> => {
    if (!actor.authority || !actor.iam || !record(body) || Array.isArray(body))
      return failure('NATIVE_BENEFITS_REQUEST_INVALID');
    if (!identifier(actor.holdingSlug))
      return failure('NATIVE_BENEFITS_REQUEST_INVALID');
    const input: NativeBenefitsReadInput = {
      holdingSlug: actor.holdingSlug,
      personal: false,
      expected: {
        companyId: actor.companyId,
        assignmentId: actor.assignmentId,
        revision: actor.authority.revision,
        assignmentVersion: actor.authority.assignmentVersion,
      },
    };
    const response = await transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      input,
      actor,
      config.timeout,
      undefined,
      {
        body: JSON.stringify(body),
        key,
        origin: incoming.get('origin') ?? '',
      },
    ).catch((error: unknown) => {
      if (error instanceof NativeBenefitsError) throw error;
      return failure('NATIVE_BENEFITS_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_BENEFITS_RESPONSE_INVALID');
    context(value.context, actor, input);
    return commandReceipt(value.data);
  };
  return { workspace, command };
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = request(
      url,
      { method, agent, headers: { ...Object.fromEntries(headers) }, signal, maxHeaderSize: 8192 },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          const values = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: status >= 200 && status <= 599 ? status : 502,
              headers: values,
            }),
          );
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', (error) => {
      agent.destroy();
      reject(error);
    });
    if (body !== undefined) outgoing.write(body);
    outgoing.end();
  });
}
