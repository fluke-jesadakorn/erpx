import 'server-only';
import { Agent, request } from 'node:http';
import { ApiError } from '@erpx/server-runtime/public/http';

export type NativeQualityDecision = Readonly<{
  canInspect: boolean;
  canRelease: boolean;
  canCancel: boolean;
  canReviewCancellation: boolean;
  canDisposition: boolean;
}>;

export type NativeQualityHold = Readonly<{
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
  decision: NativeQualityDecision;
  nextAction: string;
}>;

/// One warehouse/product stock location offered by the workspace hold form.
export type NativeQualityStock = Readonly<{
  warehouseId: string;
  productId: string;
  quantityMilli: number;
  version: number;
  productName: string;
  trackingMode: string;
  warehouseName: string;
}>;

/// One tracked lot or serial balance offered by the workspace hold form.
export type NativeQualityLot = Readonly<{
  id: string;
  warehouseId: string;
  productId: string;
  quantityMilli: number;
  balanceVersion: number;
  productName: string;
  warehouseName: string;
  trackingMode: string;
  lotCode: string | null;
  serialNumber: string | null;
  expiryDate: string | null;
  qualityHeld: boolean;
}>;

export type NativeQualityProduct = Readonly<{
  id: string;
  sku: string;
  name: string;
}>;

export type NativeQualitySpecification = Readonly<{
  id: string;
  productId: string;
  name: string;
  status: string;
  version: number;
  sku: string;
  productName: string;
}>;

export type NativeQualityInspection = Readonly<{
  id: string;
  holdId: string;
  reviewedVersion: number;
  result: 'pass' | 'fail';
  evidence: string;
  inspectedBy: string;
  createdAt: number;
  specId: string | null;
  specVersion: number | null;
}>;

export type NativeQualityMeasurement = Readonly<{
  id: string;
  inspectionId: string;
  metricId: string;
  code: string;
  label: string;
  unit: string;
  minimum: string | null;
  maximum: string | null;
  measuredValue: string;
  passed: boolean;
  ordinal: number;
}>;

export type NativeQualityCancellation = Readonly<{
  id: string;
  holdId: string;
  version: number;
  action: 'request' | 'approve' | 'reject';
  reason: string;
  actorId: string;
  createdAt: number;
}>;

export type NativeQualityCancellationPage = Readonly<{
  items: readonly NativeQualityCancellation[];
  nextBefore: number | null;
  headVersion: number;
  historyRevision: string;
}>;

export type NativeQualityCancellationCursor = Readonly<{
  before: number;
  head: number;
  revision: string;
}>;

export type NativeQualityDisposition = Readonly<{
  id: string;
  holdId: string;
  countId: string;
  action: string;
  actorId: string;
  reviewedVersion: number;
  reason: string;
  createdAt: number;
}>;

export type NativeQualityAlert = Readonly<{
  id: string;
  holdId: string;
  phase: 'due_soon' | 'overdue';
  sourceVersion: number;
  dueDate: string;
  detectedAt: number;
  version: number;
  acknowledgedBy: string | null;
  acknowledgedAt: number | null;
  reason: string | null;
}>;

export type NativeQualityMetric = Readonly<{
  id: string;
  specId: string | null;
  code: string;
  label: string;
  unit: string;
  minimum: string | null;
  maximum: string | null;
  ordinal: number;
}>;

export type NativeQualityWorkspace = Readonly<{
  holds: readonly NativeQualityHold[];
  stock: readonly NativeQualityStock[];
  lots: readonly NativeQualityLot[];
  products: readonly NativeQualityProduct[];
  specifications: readonly NativeQualitySpecification[];
  specMetrics: readonly NativeQualityMetric[];
  canHold: boolean;
  canManageSpecs: boolean;
}>;

export type NativeQualityDetail = Readonly<{
  hold: NativeQualityHold;
  inspections: readonly NativeQualityInspection[];
  measurements: readonly NativeQualityMeasurement[];
  cancellations: readonly NativeQualityCancellation[];
  dispositions: readonly NativeQualityDisposition[];
  specMetrics: readonly NativeQualityMetric[];
  alerts: readonly NativeQualityAlert[];
}>;

/// Detail source facts without the bounded retained cancellation array. The
/// caller loads the complete history through the paged contract.
export type NativeQualityDetailCore = Omit<NativeQualityDetail, 'cancellations'> &
  Readonly<{ cancellationHistory: 'paged' }>;

export type NativeQualityReceipt = Readonly<{
  id: string;
  holdId: string;
  cancellationId: string;
  status: string;
  version: number;
}>;

export type NativeQualitySpecReceipt = Readonly<{
  id: string;
  status: string;
  version: number;
}>;

export type NativeQualityInspectionReceipt = Readonly<{
  id: string;
  status: string;
  result: 'pass' | 'fail';
  version: number;
}>;

export type NativeQualityReleaseReceipt = Readonly<{
  id: string;
  status: string;
  version: number;
}>;

export type NativeQualityHoldReceipt = Readonly<{
  id: string;
  status: string;
  version: number;
}>;

export type NativeQualityDispositionReceipt = Readonly<{
  id: string;
  status: string;
  version: number;
  countId: string;
}>;

type Settings = {
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: { origin: string; holdingSlug: string }[];
  production?: boolean;
};
export type QualityTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

const reserved = new Set([3000, 3001, 3100, 3103, 5432, 5433, 5435, 8080]);
const allowed = (port: number, production: boolean) =>
  Number.isInteger(port) &&
  port >= 1 &&
  port <= 65535 &&
  !(port >= 3200 && port <= 3299) &&
  !reserved.has(port) &&
  (production || port !== 3101);
const fail = (code: string, status = 503, message = 'The native Quality reader is unavailable.'): never => {
  throw new ApiError(status, code, message);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const identifier = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && value.length <= 200;
const safeInteger = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value);
const text = (value: unknown): value is string => typeof value === 'string';
const optionalText = (value: unknown) => value === null || text(value);
const resultValue = (value: unknown): value is 'pass' | 'fail' =>
  value === 'pass' || value === 'fail';
const optionalResult = (value: unknown): value is 'pass' | 'fail' | null =>
  value === null || resultValue(value);
const optionalInteger = (value: unknown) => value === null || safeInteger(value);
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const cancellationAction = (value: unknown): value is 'request' | 'approve' | 'reject' =>
  value === 'request' || value === 'approve' || value === 'reject';
const alertPhase = (value: unknown): value is 'due_soon' | 'overdue' =>
  value === 'due_soon' || value === 'overdue';

function configuration(raw: string): Settings {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail('NATIVE_QUALITY_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    (value.production !== undefined && typeof value.production !== 'boolean') ||
    typeof value.privateBaseUrl !== 'string' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    Number(value.timeoutMs) < 250 ||
    Number(value.timeoutMs) > 30000 ||
    !Array.isArray(value.holdings) ||
    !value.holdings.length ||
    value.holdings.length > 64
  )
    return fail('NATIVE_QUALITY_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail('NATIVE_QUALITY_CONFIGURATION_INVALID');
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
    return fail('NATIVE_QUALITY_CONFIGURATION_INVALID');
  const hosts = new Set<string>();
  const slugs = new Set<string>();
  for (const holding of value.holdings) {
    if (
      !record(holding) ||
      typeof holding.origin !== 'string' ||
      typeof holding.holdingSlug !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug)
    )
      return fail('NATIVE_QUALITY_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return fail('NATIVE_QUALITY_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== holding.origin ||
      origin.username ||
      origin.password ||
      hosts.has(origin.host) ||
      slugs.has(holding.holdingSlug)
    )
      return fail('NATIVE_QUALITY_CONFIGURATION_INVALID');
    hosts.add(origin.host);
    slugs.add(holding.holdingSlug);
  }
  return value as Settings;
}

function direct(
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
    const req = request(
      url,
      {
        method,
        agent,
        headers: Object.fromEntries(headers),
        signal,
        maxHeaderSize: 8192,
      },
      (incoming) => {
        const chunks: Uint8Array[] = [];
        let total = 0;
        incoming.on('data', (chunk: Uint8Array) => {
          total += chunk.byteLength;
          if (total > 4 * 1024 * 1024) {
            incoming.destroy();
            reject(new Error('NATIVE_QUALITY_RESPONSE_LIMIT'));
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
                Object.entries(incoming.headers).flatMap(([key, value]) =>
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
    if (body !== undefined) req.write(body);
    req.end();
  });
}

function decision(value: unknown): NativeQualityDecision {
  if (
    !record(value) ||
    !boolean(value.canInspect) ||
    !boolean(value.canRelease) ||
    !boolean(value.canCancel) ||
    !boolean(value.canReviewCancellation) ||
    !boolean(value.canDisposition)
  )
    return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return {
    canInspect: value.canInspect,
    canRelease: value.canRelease,
    canCancel: value.canCancel,
    canReviewCancellation: value.canReviewCancellation,
    canDisposition: value.canDisposition,
  };
}

function hold(value: unknown): NativeQualityHold {
  if (
    !record(value) ||
    !identifier(value.id) ||
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
    !optionalResult(value.lastResult) ||
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
    return fail('NATIVE_QUALITY_RESPONSE_INVALID');
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
    decision: decision(value.decision),
    nextAction: value.nextAction,
  };
}

function stock(value: unknown): NativeQualityStock {
  if (
    !record(value) ||
    !text(value.warehouseId) ||
    !text(value.productId) ||
    !safeInteger(value.quantityMilli) ||
    !safeInteger(value.version) ||
    value.version < 1 ||
    !text(value.productName) ||
    !text(value.trackingMode) ||
    !text(value.warehouseName)
  )
    return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return {
    warehouseId: value.warehouseId,
    productId: value.productId,
    quantityMilli: value.quantityMilli,
    version: value.version,
    productName: value.productName,
    trackingMode: value.trackingMode,
    warehouseName: value.warehouseName,
  };
}

function lot(value: unknown): NativeQualityLot {
  if (
    !record(value) ||
    !identifier(value.id) ||
    !text(value.warehouseId) ||
    !text(value.productId) ||
    !safeInteger(value.quantityMilli) ||
    !safeInteger(value.balanceVersion) ||
    value.balanceVersion < 1 ||
    !text(value.productName) ||
    !text(value.warehouseName) ||
    !text(value.trackingMode) ||
    !optionalText(value.lotCode) ||
    !optionalText(value.serialNumber) ||
    !optionalText(value.expiryDate) ||
    !boolean(value.qualityHeld)
  )
    return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return {
    id: value.id,
    warehouseId: value.warehouseId,
    productId: value.productId,
    quantityMilli: value.quantityMilli,
    balanceVersion: value.balanceVersion,
    productName: value.productName,
    warehouseName: value.warehouseName,
    trackingMode: value.trackingMode,
    lotCode: value.lotCode,
    serialNumber: value.serialNumber,
    expiryDate: value.expiryDate,
    qualityHeld: value.qualityHeld,
  };
}

function product(value: unknown): NativeQualityProduct {
  if (
    !record(value) ||
    !identifier(value.id) ||
    !text(value.sku) ||
    !text(value.name)
  )
    return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return { id: value.id, sku: value.sku, name: value.name };
}

function specification(value: unknown): NativeQualitySpecification {
  if (
    !record(value) ||
    !identifier(value.id) ||
    !text(value.productId) ||
    !text(value.name) ||
    !text(value.status) ||
    !safeInteger(value.version) ||
    value.version < 1 ||
    !text(value.sku) ||
    !text(value.productName)
  )
    return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return {
    id: value.id,
    productId: value.productId,
    name: value.name,
    status: value.status,
    version: value.version,
    sku: value.sku,
    productName: value.productName,
  };
}

function inspection(value: unknown): NativeQualityInspection {
  if (
    !record(value) ||
    !identifier(value.id) ||
    !identifier(value.holdId) ||
    !safeInteger(value.reviewedVersion) ||
    !resultValue(value.result) ||
    !text(value.evidence) ||
    !text(value.inspectedBy) ||
    !safeInteger(value.createdAt) ||
    !optionalText(value.specId) ||
    !optionalInteger(value.specVersion)
  )
    return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return {
    id: value.id,
    holdId: value.holdId,
    reviewedVersion: value.reviewedVersion,
    result: value.result,
    evidence: value.evidence,
    inspectedBy: value.inspectedBy,
    createdAt: value.createdAt,
    specId: value.specId,
    specVersion: value.specVersion,
  };
}

function measurement(value: unknown): NativeQualityMeasurement {
  if (
    !record(value) ||
    !identifier(value.id) ||
    !identifier(value.inspectionId) ||
    !identifier(value.metricId) ||
    !text(value.code) ||
    !text(value.label) ||
    !text(value.unit) ||
    !optionalText(value.minimum) ||
    !optionalText(value.maximum) ||
    !text(value.measuredValue) ||
    !boolean(value.passed) ||
    !safeInteger(value.ordinal)
  )
    return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return {
    id: value.id,
    inspectionId: value.inspectionId,
    metricId: value.metricId,
    code: value.code,
    label: value.label,
    unit: value.unit,
    minimum: value.minimum,
    maximum: value.maximum,
    measuredValue: value.measuredValue,
    passed: value.passed,
    ordinal: value.ordinal,
  };
}

function cancellation(value: unknown): NativeQualityCancellation {
  if (
    !record(value) ||
    !identifier(value.id) ||
    !identifier(value.holdId) ||
    !safeInteger(value.version) ||
    !cancellationAction(value.action) ||
    !text(value.reason) ||
    !text(value.actorId) ||
    !safeInteger(value.createdAt)
  )
    return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return {
    id: value.id,
    holdId: value.holdId,
    version: value.version,
    action: value.action,
    reason: value.reason,
    actorId: value.actorId,
    createdAt: value.createdAt,
  };
}

function disposition(value: unknown): NativeQualityDisposition {
  if (
    !record(value) ||
    !identifier(value.id) ||
    !identifier(value.holdId) ||
    !text(value.countId) ||
    !text(value.action) ||
    !text(value.actorId) ||
    !safeInteger(value.reviewedVersion) ||
    !text(value.reason) ||
    !safeInteger(value.createdAt)
  )
    return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return {
    id: value.id,
    holdId: value.holdId,
    countId: value.countId,
    action: value.action,
    actorId: value.actorId,
    reviewedVersion: value.reviewedVersion,
    reason: value.reason,
    createdAt: value.createdAt,
  };
}

function alert(value: unknown): NativeQualityAlert {
  if (
    !record(value) ||
    !identifier(value.id) ||
    !identifier(value.holdId) ||
    !alertPhase(value.phase) ||
    !safeInteger(value.sourceVersion) ||
    !text(value.dueDate) ||
    !safeInteger(value.detectedAt) ||
    !safeInteger(value.version) ||
    !optionalText(value.acknowledgedBy) ||
    !optionalInteger(value.acknowledgedAt) ||
    !optionalText(value.reason)
  )
    return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return {
    id: value.id,
    holdId: value.holdId,
    phase: value.phase,
    sourceVersion: value.sourceVersion,
    dueDate: value.dueDate,
    detectedAt: value.detectedAt,
    version: value.version,
    acknowledgedBy: value.acknowledgedBy,
    acknowledgedAt: value.acknowledgedAt,
    reason: value.reason,
  };
}

function metric(value: unknown): NativeQualityMetric {
  if (
    !record(value) ||
    !identifier(value.id) ||
    !optionalText(value.specId) ||
    !text(value.code) ||
    !text(value.label) ||
    !text(value.unit) ||
    !optionalText(value.minimum) ||
    !optionalText(value.maximum) ||
    !safeInteger(value.ordinal)
  )
    return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return {
    id: value.id,
    specId: value.specId,
    code: value.code,
    label: value.label,
    unit: value.unit,
    minimum: value.minimum,
    maximum: value.maximum,
    ordinal: value.ordinal,
  };
}

function data(value: unknown): Record<string, unknown> {
  if (!record(value) || !record(value.data)) return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return value.data;
}

function rows<T>(value: unknown, parse: (row: unknown) => T): readonly T[] {
  if (!Array.isArray(value) || value.length > 5000) return fail('NATIVE_QUALITY_RESPONSE_INVALID');
  return value.map(parse);
}

export function createNativeQualityReader(raw: string, transport: QualityTransport = direct) {
  const settings = configuration(raw);
  async function send(
    method: 'GET' | 'POST',
    incoming: Pick<Headers, 'get'>,
    slug: string,
    path: string,
    body?: string,
    key?: string,
  ): Promise<Record<string, unknown>> {
    const host = incoming.get('host');
    const target = settings.holdings.find(
      (holding) => new URL(holding.origin).host === host && holding.holdingSlug === slug,
    );
    if (!target) return fail('NATIVE_QUALITY_AUTHORITY_INVALID');
    const headers = new Headers({ host: host!, accept: 'application/json' });
    for (const name of [
      'cookie',
      'x-erpx-context-company',
      'x-erpx-context-assignment',
      'x-erpx-context-revision',
      'x-erpx-context-assignment-version',
      'x-erpx-workspace-branch',
      'x-erpx-workspace-department',
      'origin',
    ]) {
      const value = incoming.get(name);
      if (value) headers.set(name, value);
    }
    if (method === 'POST') {
      headers.set('content-type', 'application/json');
      if (!key) return fail('COMMAND_KEY_REQUIRED', 422, 'Idempotency-Key is required.');
      headers.set('idempotency-key', key);
    }
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(slug)}/quality${path}`,
      settings.privateBaseUrl,
    );
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), Number(settings.timeoutMs));
    try {
      const response = await transport(
        method,
        url,
        headers,
        method === 'POST' ? body : undefined,
        controller.signal,
      );
      if (response.status !== 200) {
        const payload = await response.json().catch(() => null);
        const detail =
          record(payload) && record(payload.error) ? payload.error : null;
        const code = detail && typeof detail.code === 'string' ? detail.code : 'NATIVE_QUALITY_UNAVAILABLE';
        return fail(
          code,
          response.status >= 400 && response.status <= 599 ? response.status : 503,
          detail && typeof detail.message === 'string'
            ? detail.message
            : 'The native Quality request could not be completed.',
        );
      }
      return data(await response.json().catch(() => null));
    } catch (error) {
      if (error instanceof ApiError) throw error;
      return fail(
        controller.signal.aborted ? 'NATIVE_QUALITY_ABORTED' : 'NATIVE_QUALITY_UNAVAILABLE',
      );
    } finally {
      clearTimeout(timer);
    }
  }

  return {
    cancellationHistory: async (
      incoming: Pick<Headers, 'get'>,
      slug: string,
      id: string,
      cursor?: NativeQualityCancellationCursor,
    ): Promise<NativeQualityCancellationPage> => {
      if (!identifier(id)) return fail('NATIVE_QUALITY_REQUEST_INVALID', 422);
      if (cursor && (!safeInteger(cursor.before) || Object.is(cursor.before, -0) ||
          !safeInteger(cursor.head) || cursor.head < 1 ||
          !/^qch1:[0-9a-f]{64}$/.test(cursor.revision)))
        return fail('NATIVE_QUALITY_REQUEST_INVALID', 422);
      const query = cursor ? `?${new URLSearchParams({before: String(cursor.before), head: String(cursor.head), revision: cursor.revision})}` : '';
      const value = await send('GET', incoming, slug, `/${encodeURIComponent(id)}/cancellations${query}`);
      if (!Array.isArray(value.items) || value.items.length > 200 ||
          !safeInteger(value.headVersion) || value.headVersion < 1 ||
          !(value.nextBefore === null || (safeInteger(value.nextBefore) && !Object.is(value.nextBefore, -0))) ||
          typeof value.historyRevision !== 'string' || !/^qch1:[0-9a-f]{64}$/.test(value.historyRevision))
        return fail('NATIVE_QUALITY_RESPONSE_INVALID');
      const items = value.items.map(cancellation);
      const headVersion = value.headVersion;
      if ((cursor && items.length === 0) || new Set(items.map(item => item.id)).size !== items.length ||
          items.some((item, index) => item.holdId !== id || Object.is(item.version, -0) ||
          (index > 0 && items[index - 1].version <= item.version)) ||
          (cursor && (headVersion !== cursor.head || value.historyRevision !== cursor.revision ||
            items.some(item => item.version >= cursor.before))) ||
          (value.nextBefore !== null && (items.length === 0 || value.nextBefore !== items[items.length - 1].version)))
        return fail('NATIVE_QUALITY_RESPONSE_INVALID');
      return {items, nextBefore: value.nextBefore as number | null, headVersion, historyRevision: value.historyRevision};
    },
    workspace: async (incoming: Pick<Headers, 'get'>, slug: string): Promise<NativeQualityWorkspace> => {
      const value = await send('GET', incoming, slug, '');
      if (
        !Array.isArray(value.holds) ||
        !Array.isArray(value.stock) ||
        !Array.isArray(value.lots) ||
        !Array.isArray(value.products) ||
        !Array.isArray(value.specifications) ||
        !Array.isArray(value.specMetrics) ||
        !boolean(value.canHold) ||
        !boolean(value.canManageSpecs)
      )
        return fail('NATIVE_QUALITY_RESPONSE_INVALID');
      return {
        holds: rows(value.holds, hold),
        stock: rows(value.stock, stock),
        lots: rows(value.lots, lot),
        products: rows(value.products, product),
        specifications: rows(value.specifications, specification),
        specMetrics: rows(value.specMetrics, metric),
        canHold: value.canHold,
        canManageSpecs: value.canManageSpecs,
      };
    },
    detail: async (
      incoming: Pick<Headers, 'get'>,
      slug: string,
      id: string,
    ): Promise<NativeQualityDetail> => {
      const value = await send('GET', incoming, slug, `/${encodeURIComponent(id)}`);
      if (
        !record(value.hold) ||
        !Array.isArray(value.inspections) ||
        !Array.isArray(value.measurements) ||
        !Array.isArray(value.cancellations) ||
        !Array.isArray(value.dispositions) ||
        !Array.isArray(value.specMetrics) ||
        !Array.isArray(value.alerts)
      )
        return fail('NATIVE_QUALITY_RESPONSE_INVALID');
      return {
        hold: hold(value.hold),
        inspections: rows(value.inspections, inspection),
        measurements: rows(value.measurements, measurement),
        cancellations: rows(value.cancellations, cancellation),
        dispositions: rows(value.dispositions, disposition),
        specMetrics: rows(value.specMetrics, metric),
        alerts: rows(value.alerts, alert),
      };
    },
    detailCore: async (
      incoming: Pick<Headers, 'get'>,
      slug: string,
      id: string,
    ): Promise<NativeQualityDetailCore> => {
      const value = await send(
        'GET',
        incoming,
        slug,
        `/${encodeURIComponent(id)}/detail-core`,
      );
      if (
        !record(value.hold) ||
        !Array.isArray(value.inspections) ||
        !Array.isArray(value.measurements) ||
        !Array.isArray(value.dispositions) ||
        !Array.isArray(value.specMetrics) ||
        !Array.isArray(value.alerts) ||
        value.cancellationHistory !== 'paged'
      )
        return fail('NATIVE_QUALITY_RESPONSE_INVALID');
      return {
        hold: hold(value.hold),
        inspections: rows(value.inspections, inspection),
        measurements: rows(value.measurements, measurement),
        dispositions: rows(value.dispositions, disposition),
        specMetrics: rows(value.specMetrics, metric),
        alerts: rows(value.alerts, alert),
        cancellationHistory: 'paged',
      };
    },
    saveSpecification: async (
      incoming: Pick<Headers, 'get'>,
      slug: string,
      input: unknown,
      key: string,
    ): Promise<NativeQualitySpecReceipt> => {
      const value = await send(
        'POST',
        incoming,
        slug,
        '/specifications',
        JSON.stringify(input ?? {}),
        key,
      );
      if (
        !identifier(value.id) ||
        !text(value.status) ||
        !safeInteger(value.version) ||
        value.version < 1
      )
        return fail('NATIVE_QUALITY_RESPONSE_INVALID');
      return { id: value.id, status: value.status, version: value.version };
    },
    transitionSpecification: async (
      incoming: Pick<Headers, 'get'>,
      slug: string,
      id: string,
      input: unknown,
      key: string,
    ): Promise<NativeQualitySpecReceipt> => {
      if (!identifier(id)) return fail('NATIVE_QUALITY_REQUEST_INVALID', 422);
      const value = await send(
        'POST',
        incoming,
        slug,
        `/specifications/${encodeURIComponent(id)}`,
        JSON.stringify(input ?? {}),
        key,
      );
      if (
        !identifier(value.id) ||
        !text(value.status) ||
        !safeInteger(value.version) ||
        value.version < 1
      )
        return fail('NATIVE_QUALITY_RESPONSE_INVALID');
      return { id: value.id, status: value.status, version: value.version };
    },
    inspect: async (
      incoming: Pick<Headers, 'get'>,
      slug: string,
      id: string,
      input: unknown,
      key: string,
    ): Promise<NativeQualityInspectionReceipt> => {
      if (!identifier(id)) return fail('NATIVE_QUALITY_REQUEST_INVALID', 422);
      const value = await send(
        'POST',
        incoming,
        slug,
        `/${encodeURIComponent(id)}/inspect`,
        JSON.stringify(input ?? {}),
        key,
      );
      if (
        !identifier(value.id) ||
        !text(value.status) ||
        !resultValue(value.result) ||
        !safeInteger(value.version) ||
        value.version < 1
      )
        return fail('NATIVE_QUALITY_RESPONSE_INVALID');
      return {
        id: value.id,
        status: value.status,
        result: value.result,
        version: value.version,
      };
    },
    holdCreate: async (
      incoming: Pick<Headers, 'get'>,
      slug: string,
      input: unknown,
      key: string,
    ): Promise<NativeQualityHoldReceipt> => {
      const value = await send(
        'POST',
        incoming,
        slug,
        '',
        JSON.stringify(input ?? {}),
        key,
      );
      if (
        !identifier(value.id) ||
        !text(value.status) ||
        !safeInteger(value.version) ||
        value.version < 1
      )
        return fail('NATIVE_QUALITY_RESPONSE_INVALID');
      return { id: value.id, status: value.status, version: value.version };
    },
    release: async (
      incoming: Pick<Headers, 'get'>,
      slug: string,
      id: string,
      input: unknown,
      key: string,
    ): Promise<NativeQualityReleaseReceipt> => {
      if (!identifier(id)) return fail('NATIVE_QUALITY_REQUEST_INVALID', 422);
      const value = await send(
        'POST',
        incoming,
        slug,
        `/${encodeURIComponent(id)}/release`,
        JSON.stringify(input ?? {}),
        key,
      );
      if (
        !identifier(value.id) ||
        !text(value.status) ||
        !safeInteger(value.version) ||
        value.version < 1
      )
        return fail('NATIVE_QUALITY_RESPONSE_INVALID');
      return { id: value.id, status: value.status, version: value.version };
    },
    disposition: async (
      incoming: Pick<Headers, 'get'>,
      slug: string,
      id: string,
      input: unknown,
      key: string,
    ): Promise<NativeQualityDispositionReceipt> => {
      if (!identifier(id)) return fail('NATIVE_QUALITY_REQUEST_INVALID', 422);
      const value = await send(
        'POST',
        incoming,
        slug,
        `/${encodeURIComponent(id)}/disposition`,
        JSON.stringify(input ?? {}),
        key,
      );
      if (
        !identifier(value.id) ||
        !text(value.status) ||
        !safeInteger(value.version) ||
        value.version < 1 ||
        !identifier(value.countId)
      )
        return fail('NATIVE_QUALITY_RESPONSE_INVALID');
      return {
        id: value.id,
        status: value.status,
        version: value.version,
        countId: value.countId,
      };
    },
    cancel: async (
      incoming: Pick<Headers, 'get'>,
      slug: string,
      id: string,
      input: unknown,
      key: string,
    ): Promise<NativeQualityReceipt> => {
      const value = await send(
        'POST',
        incoming,
        slug,
        `/${encodeURIComponent(id)}/cancel`,
        JSON.stringify(input ?? {}),
        key,
      );
      if (
        !identifier(value.id) ||
        !identifier(value.holdId) ||
        !identifier(value.cancellationId) ||
        !text(value.status) ||
        !safeInteger(value.version)
      )
        return fail('NATIVE_QUALITY_RESPONSE_INVALID');
      return {
        id: value.id,
        holdId: value.holdId,
        cancellationId: value.cancellationId,
        status: value.status,
        version: value.version,
      };
    },
  };
}
