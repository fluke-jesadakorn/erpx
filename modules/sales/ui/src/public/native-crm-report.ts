import 'server-only';
import { headers } from 'next/headers';
import { defaultAnalyticsRange, type crmAnalytics, type analyticsOptions } from '@erpx/crm-server/public/crm-marketing-report';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { nativeRequest } from '@erpx/platform-runtime-server/public/native-http';
import { nativeCrmPipelineConfiguration } from './native-crm-pipeline';

const filterKeys = ['from', 'to', 'branchId', 'propertyId', 'campaignId', 'ownerId'] as const;
type Filter = typeof filterKeys[number];
type Filters = Partial<Record<Filter, string>>;

const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const unavailable = () => new ApiError(503, 'NATIVE_CRM_SALES_REPORT_UNAVAILABLE', 'The Sales CRM report is unavailable.');
const invalid = () => new ApiError(422, 'CRM_SALES_REPORT_REQUEST_INVALID', 'Review the report filters and use valid ISO dates.');
const changed = () => new ApiError(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before retrying.');
const isFilter = (key: string): key is Filter => filterKeys.some(value => value === key);

type Row = Record<string, unknown>;
type Report = Awaited<ReturnType<typeof crmAnalytics>>;
type Options = Awaited<ReturnType<typeof analyticsOptions>>;
const row = (value: unknown): value is Row => value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown): value is string => typeof value === 'string';
const integer = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value);
const iso = (value: unknown): value is string => text(value) && /^\d{4}-\d{2}-\d{2}$/.test(value);
const stringFields = (value: Row, keys: readonly string[]) => keys.every(key => text(value[key]));
const texts = (value: unknown): value is string[] => Array.isArray(value) && value.every(text);
const list = (value: unknown, item: (value: unknown) => boolean) => Array.isArray(value) && value.every(item);
const access = (value: unknown) => row(value) && text(value.revision) &&
  ['crm', 'marketing', 'hotel', 'salesRevenue', 'hotelRevenue', 'costs'].every(key => typeof value[key] === 'boolean');
const dates = (value: unknown): value is Row => row(value) && iso(value.from) && iso(value.to);
const definition = (value: unknown): value is Row => dates(value) && value.timezone === 'Asia/Bangkok' &&
  stringFields(value, ['id', 'unit', 'formula', 'source']);
const breakdown = (value: unknown): value is Row => row(value) && integer(value.count) && texts(value.leadIds);

function report(value: unknown): value is Report {
  if (!row(value) || !access(value.access) || !dates(value.filters)) return false;
  const filters = value.filters;
  return filterKeys.every(key => filters[key] === undefined || text(filters[key])) &&
    list(value.metrics, metric => definition(metric) && text(metric.label) &&
      (metric.value === null || (typeof metric.value === 'number' && Number.isFinite(metric.value))) && texts(metric.drilldown)) &&
    list(value.definitions, definition) &&
    list(value.transitions, item => breakdown(item) && text(item.action)) &&
    list(value.lossReasons, item => breakdown(item) && text(item.reason)) &&
    list(value.outcomes, item => row(item) &&
      stringFields(item, ['leadId', 'source', 'sourceId', 'currentSourceState']) && typeof item.sourceAvailable === 'boolean');
}

function options(value: unknown): value is Options {
  if (!row(value) || !access(value.access)) return false;
  return ['branches', 'properties', 'campaigns', 'owners'].every(key =>
    list(value[key], item => row(item) && stringFields(item, ['id', 'name'])));
}

function settings(actor: WorkContext, host: string | null) {
  try {
    const config = nativeCrmPipelineConfiguration(process.env.ERPX_NATIVE_CRM_PIPELINE_CONFIG ?? '');
    const base = new URL(config.privateBaseUrl);
    const port = Number(base.port);
    if (
      base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) ||
      base.origin !== config.privateBaseUrl || !port || reservedPorts.has(port) ||
      (port >= 3200 && port <= 3299) || config.timeoutMs < 250 || config.timeoutMs > 30_000
    ) throw unavailable();
    if (!host || !/^[a-z0-9.-]+(?::\d{1,5})?$/.test(host)) throw unavailable();
    const matching = config.holdings.filter(holding => holding.origin === `https://${host}`);
    if (matching.length !== 1 || matching[0]?.holdingId !== actor.holdingId || matching[0].holdingSlug !== actor.holdingSlug)
      throw unavailable();
    return { base, timeoutMs: config.timeoutMs, host };
  } catch {
    throw unavailable();
  }
}

function setMatches(current: readonly string[], expected: readonly string[]) {
  const values = new Set(current);
  return values.size === expected.length && expected.every(value => values.has(value));
}

async function readJson(response: Response) {
  if (!response.body || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    await response.body?.cancel().catch(() => undefined);
    throw unavailable();
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  const maximum = response.ok ? 2 * 1024 * 1024 : 64 * 1024;
  let size = 0;
  try {
    for (;;) {
      const chunk = await reader.read();
      if (chunk.done) break;
      size += chunk.value.byteLength;
      if (size > maximum) {
        await reader.cancel();
        throw unavailable();
      }
      chunks.push(chunk.value);
    }
    const raw: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks)));
    return raw;
  } catch {
    throw unavailable();
  } finally {
    reader.releaseLock();
  }
}

export function crmReportCompatibilityQuery(query: URLSearchParams): { kind: 'crm' | 'marketing'; filters: Filters } {
  const seen = new Set<string>();
  const filters: Filters = {};
  let kind: 'crm' | 'marketing' = 'crm';
  for (const [key, value] of query) {
    if (seen.has(key)) throw invalid();
    seen.add(key);
    if (key === 'kind' && (value === 'crm' || value === 'marketing')) kind = value;
    else if (isFilter(key)) filters[key] = value;
    else throw invalid();
  }
  return { kind, filters };
}

export function legacyCrmReportFilters(filters: Filters) {
  const defaults = defaultAnalyticsRange();
  return {
    from: filters.from ?? defaults.from,
    to: filters.to ?? defaults.to,
    branchId: filters.branchId || undefined,
    propertyId: filters.propertyId || undefined,
    campaignId: filters.campaignId || undefined,
    ownerId: filters.ownerId || undefined,
  };
}

export async function nativeCrmReport(actor: WorkContext, filters: Filters, incoming?: Pick<Headers, 'get'>) {
  const requestHeaders = incoming ?? await headers();
  const { base, timeoutMs, host } = settings(actor, requestHeaders.get('host'));
  const cookie = requestHeaders.get('cookie');
  if (cookie && Buffer.byteLength(cookie) > 8192) throw invalid();
  if (!actor.authority || !actor.iam || !/^[a-z0-9_-]{1,128}$/.test(actor.holdingSlug)) throw changed();
  const branchSlug = actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug;
  const departmentSlug = actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined;
  const authority = {
    host, accept: 'application/json', 'cache-control': 'no-cache',
    ...(cookie ? { cookie } : {}),
    ...(branchSlug ? { 'x-erpx-workspace-branch': branchSlug } : {}),
    ...(departmentSlug ? { 'x-erpx-workspace-department': departmentSlug } : {}),
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  };

  const url = new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/reports/sales-crm`, base);
  for (const key of filterKeys) {
    const value = filters[key];
    if (value !== undefined) url.searchParams.set(key, value);
  }
  if (url.search.length > 4097) throw invalid();
  let response: Response;
  try {
    response = await nativeRequest(url, { headers: authority }, timeoutMs);
  } catch {
    throw unavailable();
  }
  const raw = await readJson(response);
  if (response.status !== 200) {
    const error = row(raw) && row(raw.error) ? raw.error : undefined;
    if (response.status >= 400 && response.status < 500 && text(error?.code) && error.code.length <= 200 &&
      text(error.message) && error.message.length <= 2000)
      throw new ApiError(response.status, error.code, error.message);
    throw unavailable();
  }
  if (!row(raw) || !row(raw.context) || !report(raw.data) || !options(raw.options)) throw unavailable();
  const { context, data, options: filterOptions } = raw;
  if (
    context.userId !== actor.userId || context.holdingId !== actor.holdingId ||
    context.companyId !== actor.companyId || context.branchId !== actor.branchId ||
    context.departmentId !== actor.departmentId || context.branchSlug !== (branchSlug ?? null) ||
    context.departmentSlug !== (departmentSlug ?? null) || context.assignmentId !== actor.assignmentId ||
    context.assignmentVersion !== actor.authority.assignmentVersion || context.revision !== actor.authority.revision ||
    context.role !== actor.role || context.assignmentScopeKind !== actor.assignmentScope ||
    !integer(context.expiresAt) || context.expiresAt <= 0 ||
    !texts(context.modules) || !setMatches(context.modules, actor.modules) ||
    !texts(context.permissions) || !setMatches(context.permissions, actor.iam.permissions)
  ) throw changed();
  return { data, options: filterOptions };
}
