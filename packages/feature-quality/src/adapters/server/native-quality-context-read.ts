import 'server-only';
import { headers } from 'next/headers';
import { Agent, request } from 'node:http';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { WorkItem } from '@erpx/feature-work_projection/lib/work-item';

/** Native Quality work/detail_core fact accepted by internal consumers. The
 * detail hold carries the same field names the retained `QualityRow` mirror
 * exposed, so callers do not keep a second policy or SQL source. */
export type NativeQualityWorkContextHold = Readonly<{
  id: string;
  companyId: string;
  branchId: string;
  warehouseId: string;
  productId: string;
  lotId: string | null;
  reason: string;
  dueDate: string;
  status: string;
  createdBy: string;
  lastInspector: string | null;
  lastResult: 'pass' | 'fail' | null;
  releasedBy: string | null;
  releaseReason: string | null;
  releasedAt: number | null;
  version: number;
  createdAt: number;
  updatedAt: number;
  specId: string | null;
  specVersion: number | null;
  quantityScope: string;
  heldQuantityMilli: number | null;
  sourceType: string | null;
  sourceId: string | null;
  cancelRequestedBy: string | null;
  cancelReason: string | null;
  cancelledBy: string | null;
  cancelledAt: number | null;
  dispositionKind: string | null;
  dispositionReason: string | null;
  dispositionRequestedBy: string | null;
  dispositionCountId: string | null;
  dispositionQuantityMilli: number | null;
  disposedBy: string | null;
  disposedAt: number | null;
  inspectionCount: number;
  productName: string;
  productUnit: string;
  trackingMode: string;
  warehouseName: string;
  lotCode: string | null;
  serialNumber: string | null;
  expiryDate: string | null;
  eligibleCount: number;
  nextAction: string;
}>;

export type NativeQualityWorkContextDetail = Readonly<{
  hold: NativeQualityWorkContextHold;
  inspections: readonly Record<string, unknown>[];
  measurements: readonly Record<string, unknown>[];
  dispositions: readonly Record<string, unknown>[];
  specMetrics: readonly Record<string, unknown>[];
  alerts: readonly Record<string, unknown>[];
  cancellationHistory: 'paged';
}>;

type Settings = {
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: { origin: string; holdingSlug: string }[];
  production?: boolean;
};

type Transport = (url: URL, headers: Headers, signal: AbortSignal) => Promise<Response>;

const reserved = new Set([3000, 3001, 3100, 3103, 5432, 5433, 5435, 8080]);
const allowed = (port: number, production: boolean) =>
  Number.isInteger(port) &&
  port >= 1 &&
  port <= 65535 &&
  !(port >= 3200 && port <= 3299) &&
  !reserved.has(port) &&
  (production || port !== 3101);

const fail = (
  code: string,
  status = 503,
  message = 'The native Quality work-context reader is unavailable.',
): never => {
  throw new ApiError(status, code, message);
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown): value is string => typeof value === 'string' && value.length > 0;
const optionalText = (value: unknown): value is string | null => value === null || typeof value === 'string';
const safeInteger = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value);
const optionalInteger = (value: unknown): value is number | null => value === null || safeInteger(value);
const boundedList = (value: unknown): value is Record<string, unknown>[] =>
  Array.isArray(value) && value.length <= 5000 && value.every(record);

function configuration(raw: string): Settings {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail('NATIVE_QUALITY_WORK_CONTEXT_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    (value.production !== undefined && typeof value.production !== 'boolean') ||
    typeof value.privateBaseUrl !== 'string' ||
    !safeInteger(value.timeoutMs) ||
    Number(value.timeoutMs) < 250 ||
    Number(value.timeoutMs) > 30000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length === 0 ||
    value.holdings.length > 64
  )
    return fail('NATIVE_QUALITY_WORK_CONTEXT_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail('NATIVE_QUALITY_WORK_CONTEXT_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    !allowed(port, value.production === true) ||
    base.origin !== value.privateBaseUrl ||
    base.username ||
    base.password
  )
    return fail('NATIVE_QUALITY_WORK_CONTEXT_CONFIGURATION_INVALID');
  const hosts = new Set<string>();
  const slugs = new Set<string>();
  for (const holding of value.holdings) {
    if (
      !record(holding) ||
      typeof holding.origin !== 'string' ||
      typeof holding.holdingSlug !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug)
    )
      return fail('NATIVE_QUALITY_WORK_CONTEXT_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return fail('NATIVE_QUALITY_WORK_CONTEXT_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== holding.origin ||
      origin.username ||
      origin.password ||
      hosts.has(origin.host) ||
      slugs.has(holding.holdingSlug)
    )
      return fail('NATIVE_QUALITY_WORK_CONTEXT_CONFIGURATION_INVALID');
    hosts.add(origin.host);
    slugs.add(holding.holdingSlug);
  }
  return value as Settings;
}

function direct(
  url: URL,
  headers: Headers,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const req = request(
      url,
      { method: 'GET', agent, headers: Object.fromEntries(headers), signal, maxHeaderSize: 8192 },
      (incoming) => {
        const chunks: Uint8Array[] = [];
        let total = 0;
        incoming.on('data', (chunk: Uint8Array) => {
          total += chunk.byteLength;
          if (total > 4 * 1024 * 1024) {
            incoming.destroy();
            reject(new Error('NATIVE_QUALITY_WORK_CONTEXT_RESPONSE_LIMIT'));
            return;
          }
          chunks.push(chunk);
        });
        incoming.on('end', () => {
          const bytes = new Uint8Array(total);
          let offset = 0;
          for (const chunk of chunks) {
            bytes.set(chunk, offset);
            offset += chunk.byteLength;
          }
          resolve(
            new Response(bytes, {
              status: incoming.statusCode ?? 0,
              headers: new Headers(
                Object.entries(incoming.headers).flatMap<[string, string]>(([key, value]) =>
                  value === undefined
                    ? []
                    : [[key, Array.isArray(value) ? value.join(', ') : value]],
                ),
              ),
            }),
          );
        });
        incoming.on('error', reject);
      },
    );
    req.on('error', reject);
    req.end();
  });
}

function hold(value: unknown): NativeQualityWorkContextHold {
  if (
    !record(value) ||
    !text(value.id) ||
    !text(value.companyId) ||
    !text(value.branchId) ||
    !text(value.warehouseId) ||
    !text(value.productId) ||
    !optionalText(value.lotId) ||
    !text(value.reason) ||
    !text(value.dueDate) ||
    !text(value.status) ||
    !text(value.createdBy) ||
    !optionalText(value.lastInspector) ||
    !(value.lastResult === 'pass' || value.lastResult === 'fail' || value.lastResult === null) ||
    !optionalText(value.releasedBy) ||
    !optionalText(value.releaseReason) ||
    !optionalInteger(value.releasedAt) ||
    !safeInteger(value.version) ||
    value.version < 1 ||
    !safeInteger(value.createdAt) ||
    !safeInteger(value.updatedAt) ||
    !optionalText(value.specId) ||
    !optionalInteger(value.specVersion) ||
    !text(value.quantityScope) ||
    !optionalInteger(value.heldQuantityMilli) ||
    !optionalText(value.sourceType) ||
    !optionalText(value.sourceId) ||
    !optionalText(value.cancelRequestedBy) ||
    !optionalText(value.cancelReason) ||
    !optionalText(value.cancelledBy) ||
    !optionalInteger(value.cancelledAt) ||
    !optionalText(value.dispositionKind) ||
    !optionalText(value.dispositionReason) ||
    !optionalText(value.dispositionRequestedBy) ||
    !optionalText(value.dispositionCountId) ||
    !optionalInteger(value.dispositionQuantityMilli) ||
    !optionalText(value.disposedBy) ||
    !optionalInteger(value.disposedAt) ||
    !safeInteger(value.inspectionCount) ||
    !text(value.productName) ||
    !text(value.productUnit) ||
    !text(value.trackingMode) ||
    !text(value.warehouseName) ||
    !optionalText(value.lotCode) ||
    !optionalText(value.serialNumber) ||
    !optionalText(value.expiryDate) ||
    !safeInteger(value.eligibleCount) ||
    !text(value.nextAction)
  )
    return fail('NATIVE_QUALITY_WORK_CONTEXT_RESPONSE_INVALID', 502);
  return {
    id: value.id,
    companyId: value.companyId,
    branchId: value.branchId,
    warehouseId: value.warehouseId,
    productId: value.productId,
    lotId: value.lotId,
    reason: value.reason,
    dueDate: value.dueDate,
    status: value.status,
    createdBy: value.createdBy,
    lastInspector: value.lastInspector,
    lastResult: value.lastResult,
    releasedBy: value.releasedBy,
    releaseReason: value.releaseReason,
    releasedAt: value.releasedAt,
    version: value.version,
    createdAt: value.createdAt,
    updatedAt: value.updatedAt,
    specId: value.specId,
    specVersion: value.specVersion,
    quantityScope: value.quantityScope,
    heldQuantityMilli: value.heldQuantityMilli,
    sourceType: value.sourceType,
    sourceId: value.sourceId,
    cancelRequestedBy: value.cancelRequestedBy,
    cancelReason: value.cancelReason,
    cancelledBy: value.cancelledBy,
    cancelledAt: value.cancelledAt,
    dispositionKind: value.dispositionKind,
    dispositionReason: value.dispositionReason,
    dispositionRequestedBy: value.dispositionRequestedBy,
    dispositionCountId: value.dispositionCountId,
    dispositionQuantityMilli: value.dispositionQuantityMilli,
    disposedBy: value.disposedBy,
    disposedAt: value.disposedAt,
    inspectionCount: value.inspectionCount,
    productName: value.productName,
    productUnit: value.productUnit,
    trackingMode: value.trackingMode,
    warehouseName: value.warehouseName,
    lotCode: value.lotCode,
    serialNumber: value.serialNumber,
    expiryDate: value.expiryDate,
    eligibleCount: value.eligibleCount,
    nextAction: value.nextAction,
  };
}

function workItem(value: unknown): WorkItem {
  if (
    !record(value) ||
    !text(value.id) ||
    value.type !== 'quality_hold' ||
    !record(value.source) ||
    value.source.kind !== 'quality_hold' ||
    value.source.id !== value.id ||
    !text(value.source.href) ||
    value.source.label !== 'Quality hold' ||
    !safeInteger(value.source.version) ||
    (value.actionKind !== 'work' && value.actionKind !== 'approval') ||
    !text(value.title) ||
    typeof value.detail !== 'string' ||
    !text(value.href) ||
    !text(value.status) ||
    !safeInteger(value.updatedAt) ||
    (value.queue !== 'todo' && value.queue !== 'waiting' && value.queue !== 'completed') ||
    typeof value.canAct !== 'boolean' ||
    !(value.dueAt === null || safeInteger(value.dueAt)) ||
    !text(value.responsibleQueue) ||
    !text(value.nextAction) ||
    !(value.blocker === null || typeof value.blocker === 'string') ||
    !Array.isArray(value.relationships) ||
    value.relationships.length > 4 ||
    !value.relationships.every((entry) =>
      ['requested', 'assigned', 'approval', 'team'].includes(entry),
    )
  )
    return fail('NATIVE_QUALITY_WORK_CONTEXT_RESPONSE_INVALID', 502);
  return {
    source: {
      kind: 'quality_hold',
      id: value.source.id,
      href: value.source.href,
      label: 'Quality hold',
      version: value.source.version,
    } as WorkItem['source'],
    actionKind: value.actionKind,
    canAct: value.canAct,
    id: value.id,
    type: 'quality_hold',
    title: value.title,
    detail: value.detail,
    href: value.href,
    status: value.status,
    updatedAt: value.updatedAt,
    queue: value.queue,
    responsibleQueue: value.responsibleQueue,
    nextAction: value.nextAction,
    dueAt: value.dueAt as number | null,
    blocker: value.blocker as string | null,
    relationships: value.relationships as WorkItem['relationships'],
  };
}

function sameSet(left: readonly string[], right: unknown): boolean {
  return (
    Array.isArray(right) &&
    right.every((value) => typeof value === 'string') &&
    left.length === right.length &&
    new Set(left).size === new Set(right).size &&
    left.every((value) => right.includes(value))
  );
}

function verifyContext(actor: WorkContext, value: unknown): void {
  if (!actor.authority || !actor.iam) return fail('WORK_CONTEXT_CHANGED', 409);
  const expected: Record<string, unknown> = {
    holdingId: actor.holdingId,
    userId: actor.userId,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    branchSlug: actor.scope.kind === 'holding' ? null : actor.scope.branchSlug ?? null,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : null,
    assignmentId: actor.assignmentId,
    assignmentVersion: actor.authority.assignmentVersion,
    revision: actor.authority.revision,
    role: actor.role,
    assignmentScopeKind: actor.assignmentScope,
    modules: actor.modules,
    permissions: actor.iam.permissions,
  };
  if (!record(value)) return fail('WORK_CONTEXT_CHANGED', 409);
  for (const [key, item] of Object.entries(expected)) {
    if (key === 'modules' || key === 'permissions') {
      if (!sameSet(item as string[], value[key])) return fail('WORK_CONTEXT_CHANGED', 409);
    } else if (value[key] !== item) {
      return fail('WORK_CONTEXT_CHANGED', 409);
    }
  }
  if (!safeInteger(value.expiresAt) || Number(value.expiresAt) <= Date.now())
    return fail('WORK_CONTEXT_CHANGED', 409);
}

export function createNativeQualityWorkContextReader(
  raw: string | undefined,
  transport: Transport = direct,
) {
  if (raw === undefined)
    return fail('NATIVE_QUALITY_WORK_CONTEXT_UNAVAILABLE');
  const settings = configuration(raw);

  async function send(actor: WorkContext, path: string): Promise<Record<string, unknown>> {
    const host = (await headers()).get('host');
    const target = settings.holdings.find(
      (holding) => new URL(holding.origin).host === host && holding.holdingSlug === actor.holdingSlug,
    );
    if (!target) return fail('NATIVE_QUALITY_AUTHORITY_INVALID', 409);
    if (!actor.authority || !actor.iam) return fail('WORK_CONTEXT_CHANGED', 409);
    const incoming = await headers();
    const outbound = new Headers({
      host: host!,
      accept: 'application/json',
      'cache-control': 'no-cache',
      'x-erpx-context-company': actor.companyId,
      'x-erpx-context-assignment': actor.assignmentId,
      'x-erpx-context-revision': String(actor.authority.revision),
      'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
    });
    for (const name of ['cookie', 'origin']) {
      const value = incoming.get(name);
      if (value) outbound.set(name, value);
    }
    if (actor.scope.kind !== 'holding' && actor.scope.branchSlug)
      outbound.set('x-erpx-workspace-branch', actor.scope.branchSlug);
    if (actor.scope.kind === 'department' && actor.scope.departmentSlug)
      outbound.set('x-erpx-workspace-department', actor.scope.departmentSlug);
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/quality/work-context${path}`,
      settings.privateBaseUrl,
    );
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), settings.timeoutMs);
    try {
      const response = await transport(url, outbound, controller.signal);
      if (response.status !== 200) {
        const payload = await response.json().catch(() => null);
        const detail = record(payload) && record(payload.error) ? payload.error : null;
        const code = detail && typeof detail.code === 'string' ? detail.code : 'NATIVE_QUALITY_WORK_CONTEXT_UNAVAILABLE';
        return fail(
          code,
          response.status >= 400 && response.status <= 599 ? response.status : 503,
          detail && typeof detail.message === 'string' ? detail.message : undefined,
        );
      }
      const body: unknown = await response.json().catch(() => null);
      if (!record(body) || !record(body.data)) return fail('NATIVE_QUALITY_WORK_CONTEXT_RESPONSE_INVALID', 502);
      verifyContext(actor, body.context);
      return record(body.data) ? body.data : fail('NATIVE_QUALITY_WORK_CONTEXT_RESPONSE_INVALID', 502);
    } catch (error) {
      if (error instanceof ApiError) throw error;
      return fail(
        controller.signal.aborted ? 'NATIVE_QUALITY_WORK_CONTEXT_ABORTED' : 'NATIVE_QUALITY_WORK_CONTEXT_UNAVAILABLE',
      );
    } finally {
      clearTimeout(timer);
    }
  }

  return {
    work: async (actor: WorkContext): Promise<{ items: WorkItem[] }> => {
      const data = await send(actor, '/work');
      if (!Array.isArray(data.items) || data.items.length > 5000)
        return fail('NATIVE_QUALITY_WORK_CONTEXT_RESPONSE_INVALID', 502);
      return { items: data.items.map(workItem) };
    },
    detailCore: async (actor: WorkContext, id: string): Promise<NativeQualityWorkContextDetail> => {
      if (!text(id)) return fail('NATIVE_QUALITY_WORK_CONTEXT_REQUEST_INVALID', 422);
      const data = await send(actor, `/holds/${encodeURIComponent(id)}/detail-core`);
      if (
        !record(data.hold) ||
        !boundedList(data.inspections) ||
        !boundedList(data.measurements) ||
        !boundedList(data.dispositions) ||
        !boundedList(data.specMetrics) ||
        !boundedList(data.alerts) ||
        data.cancellationHistory !== 'paged'
      )
        return fail('NATIVE_QUALITY_WORK_CONTEXT_RESPONSE_INVALID', 502);
      return {
        hold: hold(data.hold),
        inspections: data.inspections,
        measurements: data.measurements,
        dispositions: data.dispositions,
        specMetrics: data.specMetrics,
        alerts: data.alerts,
        cancellationHistory: 'paged',
      };
    },
  };
}

/** Request-bound wrapper used by internal consumers. Missing or invalid
 * configuration fails closed; no TypeScript SQL fallback is retained. */
export async function nativeQualityWork(actor: WorkContext): Promise<{ items: WorkItem[] }> {
  return createNativeQualityWorkContextReader(process.env.ERPX_NATIVE_QUALITY_CONFIG).work(actor);
}

export async function nativeQualityHoldDetail(
  actor: WorkContext,
  id: string,
): Promise<NativeQualityWorkContextDetail> {
  return createNativeQualityWorkContextReader(
    process.env.ERPX_NATIVE_QUALITY_CONFIG,
  ).detailCore(actor, id);
}
