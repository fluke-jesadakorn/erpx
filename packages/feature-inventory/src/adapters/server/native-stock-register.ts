import 'server-only';
import {Agent, request} from 'node:http';
import {headers} from 'next/headers';
import {allowedPrivateLoopbackPort} from '@erpx/native-sdk/loopback';
import type {
  NativeAvailabilityContext,
  NativeAvailabilityRow,
} from './native-availability';

export type NativeStockLot = {
  id: string;
  warehouse_id: string;
  product_id: string;
  sku: string;
  tracking_mode: string;
  warehouse_name: string;
  lot_code: string | null;
  serial_number: string | null;
  manufactured_date: string | null;
  expiry_date: string | null;
  status: string;
  quantity_milli: number;
  value_minor: number;
  version: number;
  quality_hold_milli: number;
  quality_status: string | null;
};

export type NativeStockCount = {
  id: string;
  number: string;
  status: string;
  version: number;
  count_date: string;
  reason: string;
  warehouse_id: string;
  warehouse_name: string;
  net_value_minor: number;
  posted_at: number | null;
  journal_id: string | null;
  initial_physical_source: boolean;
  created_at: number;
  updated_at: number;
};

export type NativeStockTotals = {
  tracked_quantity_milli: number;
  tracked_value_minor: number;
  lot_quantity_milli: number;
  lot_value_minor: number;
  consistent: boolean;
};

export type NativeStockRegister = {
  stock: NativeAvailabilityRow[];
  lots: NativeStockLot[];
  counts: NativeStockCount[];
  totals: NativeStockTotals;
  currency: string;
};

export type NativeReceiptOrder = {
  id: string;
  number: string;
  kind: string;
  state: string;
  version: number;
  warehouse_id: string;
  branch_id: string;
  department_id: string | null;
  order_date: string;
  currency: string;
  party_name: string;
};

export type NativeReceiptStock = {
  quantity_milli: number;
  value_minor: number;
  version: number;
  reserved_milli: number;
  quarantined_milli: number;
};

export type NativeReceiptLot = {
  id: string;
  lot_code: string | null;
  serial_number: string | null;
  status: string;
  quantity_milli: number;
  value_minor: number;
  version: number;
  quality_hold_milli: number;
  quality_status: string | null;
};

export type NativeReceiptLine = {
  id: string;
  product_id: string;
  sku: string;
  description: string;
  unit: string;
  tracking_mode: string;
  quantity_milli: number;
  fulfilled_milli: number;
  remaining_milli: number;
  price_minor: number;
  tax_bps: number;
  ordinal: number;
  stock: NativeReceiptStock | null;
  lots: NativeReceiptLot[];
  lots_truncated: boolean;
};

export type NativeReceiptSource = {
  company_id: string;
  order: NativeReceiptOrder;
  lines: NativeReceiptLine[];
  can_receive: boolean;
};

type Settings = {
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: {origin: string; holdingSlug: string; holdingId: string}[];
  production?: boolean;
};
type Transport = (url: URL, headers: Headers, signal: AbortSignal) => Promise<Response>;

export class NativeStockRegisterError extends Error {
  constructor(
    readonly code: string,
    readonly status?: number,
  ) {
    super('The native Inventory stock register is unavailable.');
  }
}

const fail = (code: string, status?: number): never => {
  throw new NativeStockRegisterError(code, status);
};
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 200): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
const nullableText = (value: unknown, max = 200): value is string | null =>
  value === null || text(value, max);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const signedInteger = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value);
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';

function configuration(raw: string): Settings {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail('NATIVE_INVENTORY_STOCK_CONFIGURATION_INVALID');
  }
  if (
    !object(value) ||
    (value.production !== undefined && typeof value.production !== 'boolean') ||
    typeof value.privateBaseUrl !== 'string' ||
    !integer(value.timeoutMs, 250) ||
    value.timeoutMs > 30000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  ) {
    return fail('NATIVE_INVENTORY_STOCK_CONFIGURATION_INVALID');
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail('NATIVE_INVENTORY_STOCK_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    !allowedPrivateLoopbackPort(port, value.production === true) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin
  ) {
    return fail('NATIVE_INVENTORY_STOCK_CONFIGURATION_INVALID');
  }
  const hosts = new Set<string>();
  const slugs = new Set<string>();
  for (const holding of value.holdings) {
    if (
      !object(holding) ||
      typeof holding.origin !== 'string' ||
      typeof holding.holdingSlug !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug) ||
      !text(holding.holdingId, 200)
    ) {
      return fail('NATIVE_INVENTORY_STOCK_CONFIGURATION_INVALID');
    }
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return fail('NATIVE_INVENTORY_STOCK_CONFIGURATION_INVALID');
    }
    if (origin.origin !== holding.origin || origin.protocol !== 'https:') {
      return fail('NATIVE_INVENTORY_STOCK_CONFIGURATION_INVALID');
    }
    if (hosts.has(origin.host) || slugs.has(holding.holdingSlug)) {
      return fail('NATIVE_INVENTORY_STOCK_CONFIGURATION_INVALID');
    }
    hosts.add(origin.host);
    slugs.add(holding.holdingSlug);
  }
  return value as Settings;
}

function direct(url: URL, headers: Headers, signal: AbortSignal): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const req = request(
      url,
      {method: 'GET', agent, headers: Object.fromEntries(headers), signal},
      (response) => {
        const chunks: Buffer[] = [];
        response.on('data', (chunk) => chunks.push(Buffer.from(chunk)));
        response.on('end', () =>
          resolve(
            new Response(Buffer.concat(chunks), {
              status: response.statusCode ?? 500,
              headers: response.headers as HeadersInit,
            }),
          ),
        );
      },
    );
    req.on('error', reject);
    req.end();
  });
}

function requestHeaders(
  incoming: Pick<Headers, 'get'>,
  actor: NativeAvailabilityContext,
): Headers {
  const host = incoming.get('host');
  if (!host) return fail('NATIVE_INVENTORY_STOCK_AUTHORITY_INVALID');
  const out = new Headers({host, accept: 'application/json', 'cache-control': 'no-cache'});
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192) {
      return fail('NATIVE_INVENTORY_STOCK_COOKIE_LIMIT');
    }
    out.set('cookie', cookie);
  }
  const origin = incoming.get('origin');
  if (origin) out.set('origin', origin);
  out.set('x-erpx-context-company', actor.companyId);
  out.set('x-erpx-context-assignment', actor.assignmentId);
  out.set('x-erpx-context-revision', String(actor.revision));
  out.set('x-erpx-context-assignment-version', String(actor.assignmentVersion));
  if (actor.branchSlug) out.set('x-erpx-workspace-branch', actor.branchSlug);
  if (actor.departmentSlug) out.set('x-erpx-workspace-department', actor.departmentSlug);
  return out;
}

function contextMatches(value: unknown, actor: NativeAvailabilityContext): boolean {
  if (!object(value)) return false;
  return (
    value.userId === actor.userId &&
    value.holdingId === actor.holdingId &&
    value.companyId === actor.companyId &&
    value.branchId === actor.branchId &&
    value.departmentId === actor.departmentId &&
    value.assignmentId === actor.assignmentId &&
    value.revision === actor.revision &&
    value.assignmentVersion === actor.assignmentVersion &&
    integer(value.expiresAt, 1)
  );
}

function stockRow(value: unknown): NativeAvailabilityRow {
  if (
    !object(value) ||
    !text(value.warehouseId) ||
    !text(value.productId) ||
    typeof value.sku !== 'string' ||
    typeof value.name !== 'string' ||
    typeof value.unit !== 'string' ||
    typeof value.trackingMode !== 'string' ||
    typeof value.warehouseName !== 'string' ||
    !integer(value.quantityMilli) ||
    !integer(value.reservedMilli) ||
    !integer(value.quarantinedMilli) ||
    !integer(value.inTransitMilli) ||
    !integer(value.availableMilli) ||
    !integer(value.valueMinor) ||
    !integer(value.version, 1)
  ) {
    return fail('NATIVE_INVENTORY_STOCK_RESPONSE_INVALID');
  }
  if (
    value.availableMilli !==
    Math.max(0, value.quantityMilli - Math.max(value.reservedMilli, value.quarantinedMilli))
  ) {
    return fail('NATIVE_INVENTORY_STOCK_RESPONSE_INVALID');
  }
  return {
    warehouse_id: value.warehouseId,
    product_id: value.productId,
    sku: value.sku,
    name: value.name,
    unit: value.unit,
    tracking_mode: value.trackingMode,
    warehouse_name: value.warehouseName,
    quantity_milli: value.quantityMilli,
    reserved_milli: value.reservedMilli,
    quarantined_milli: value.quarantinedMilli,
    in_transit_milli: value.inTransitMilli,
    available_milli: value.availableMilli,
    value_minor: value.valueMinor,
    version: value.version,
  };
}

function lotRow(value: unknown): NativeStockLot {
  if (
    !object(value) ||
    !text(value.id) ||
    !text(value.warehouseId) ||
    !text(value.productId) ||
    typeof value.sku !== 'string' ||
    typeof value.trackingMode !== 'string' ||
    typeof value.warehouseName !== 'string' ||
    !nullableText(value.lotCode) ||
    !nullableText(value.serialNumber) ||
    !nullableText(value.manufacturedDate, 40) ||
    !nullableText(value.expiryDate, 40) ||
    !text(value.status, 40) ||
    !integer(value.quantityMilli) ||
    !integer(value.valueMinor) ||
    !integer(value.version, 1) ||
    !integer(value.qualityHoldMilli) ||
    value.qualityHoldMilli > value.quantityMilli ||
    !nullableText(value.qualityStatus, 40)
  ) {
    return fail('NATIVE_INVENTORY_STOCK_RESPONSE_INVALID');
  }
  if (value.trackingMode === 'lot' ? !!value.serialNumber : !value.serialNumber) {
    return fail('NATIVE_INVENTORY_STOCK_RESPONSE_INVALID');
  }
  return {
    id: value.id,
    warehouse_id: value.warehouseId,
    product_id: value.productId,
    sku: value.sku,
    tracking_mode: value.trackingMode,
    warehouse_name: value.warehouseName,
    lot_code: value.lotCode,
    serial_number: value.serialNumber,
    manufactured_date: value.manufacturedDate,
    expiry_date: value.expiryDate,
    status: value.status,
    quantity_milli: value.quantityMilli,
    value_minor: value.valueMinor,
    version: value.version,
    quality_hold_milli: value.qualityHoldMilli,
    quality_status: value.qualityStatus,
  };
}

function countRow(value: unknown): NativeStockCount {
  if (
    !object(value) ||
    !text(value.id) ||
    !text(value.number) ||
    !text(value.status, 40) ||
    !integer(value.version, 1) ||
    typeof value.countDate !== 'string' ||
    typeof value.reason !== 'string' ||
    !text(value.warehouseId) ||
    typeof value.warehouseName !== 'string' ||
    !signedInteger(value.netValueMinor) ||
    (value.postedAt !== null && !integer(value.postedAt)) ||
    !nullableText(value.journalId) ||
    !boolean(value.initialPhysicalSource) ||
    !integer(value.createdAt) ||
    !integer(value.updatedAt)
  ) {
    return fail('NATIVE_INVENTORY_STOCK_RESPONSE_INVALID');
  }
  return {
    id: value.id,
    number: value.number,
    status: value.status,
    version: value.version,
    count_date: value.countDate,
    reason: value.reason,
    warehouse_id: value.warehouseId,
    warehouse_name: value.warehouseName,
    net_value_minor: value.netValueMinor,
    posted_at: value.postedAt,
    journal_id: value.journalId,
    initial_physical_source: value.initialPhysicalSource,
    created_at: value.createdAt,
    updated_at: value.updatedAt,
  };
}

function totals(value: unknown): NativeStockTotals {
  if (
    !object(value) ||
    !integer(value.trackedQuantityMilli) ||
    !integer(value.trackedValueMinor) ||
    !integer(value.lotQuantityMilli) ||
    !integer(value.lotValueMinor) ||
    !boolean(value.consistent)
  ) {
    return fail('NATIVE_INVENTORY_STOCK_RESPONSE_INVALID');
  }
  const expected =
    value.trackedQuantityMilli === value.lotQuantityMilli &&
    value.trackedValueMinor === value.lotValueMinor;
  if (value.consistent !== expected) {
    return fail('NATIVE_INVENTORY_STOCK_RESPONSE_INVALID');
  }
  return {
    tracked_quantity_milli: value.trackedQuantityMilli,
    tracked_value_minor: value.trackedValueMinor,
    lot_quantity_milli: value.lotQuantityMilli,
    lot_value_minor: value.lotValueMinor,
    consistent: value.consistent,
  };
}

function validateRegister(value: unknown, actor: NativeAvailabilityContext): NativeStockRegister {
  if (
    !object(value) ||
    !contextMatches(value.context, actor) ||
    !object(value.data) ||
    !Array.isArray(value.data.stock) ||
    !Array.isArray(value.data.lots) ||
    !Array.isArray(value.data.counts) ||
    value.data.stock.length > 1000 ||
    value.data.lots.length > 2000 ||
    value.data.counts.length > 200 ||
    !text(value.data.currency, 3) ||
    value.data.currency.length !== 3
  ) {
    return fail('NATIVE_INVENTORY_STOCK_RESPONSE_INVALID');
  }
  return {
    stock: value.data.stock.map(stockRow),
    lots: value.data.lots.map(lotRow),
    counts: value.data.counts.map(countRow),
    totals: totals(value.data.totals),
    currency: value.data.currency,
  };
}

function receiptStock(value: unknown): NativeReceiptStock {
  if (
    !object(value) ||
    !integer(value.quantityMilli) ||
    !integer(value.valueMinor) ||
    !integer(value.version, 1) ||
    !integer(value.reservedMilli) ||
    !integer(value.quarantinedMilli)
  ) {
    return fail('NATIVE_INVENTORY_RECEIPT_RESPONSE_INVALID');
  }
  return {
    quantity_milli: value.quantityMilli,
    value_minor: value.valueMinor,
    version: value.version,
    reserved_milli: value.reservedMilli,
    quarantined_milli: value.quarantinedMilli,
  };
}

function receiptLot(value: unknown): NativeReceiptLot {
  if (
    !object(value) ||
    !text(value.id) ||
    !nullableText(value.lotCode) ||
    !nullableText(value.serialNumber) ||
    !text(value.status, 40) ||
    !integer(value.quantityMilli) ||
    !integer(value.valueMinor) ||
    !integer(value.version, 1) ||
    !integer(value.qualityHoldMilli) ||
    value.qualityHoldMilli > value.quantityMilli ||
    !nullableText(value.qualityStatus, 40)
  ) {
    return fail('NATIVE_INVENTORY_RECEIPT_RESPONSE_INVALID');
  }
  return {
    id: value.id,
    lot_code: value.lotCode,
    serial_number: value.serialNumber,
    status: value.status,
    quantity_milli: value.quantityMilli,
    value_minor: value.valueMinor,
    version: value.version,
    quality_hold_milli: value.qualityHoldMilli,
    quality_status: value.qualityStatus,
  };
}

function receiptLine(value: unknown): NativeReceiptLine {
  if (
    !object(value) ||
    !text(value.id) ||
    !text(value.productId) ||
    typeof value.sku !== 'string' ||
    typeof value.description !== 'string' ||
    typeof value.unit !== 'string' ||
    !['none', 'lot', 'serial'].includes(value.trackingMode as string) ||
    !integer(value.quantityMilli) ||
    !integer(value.fulfilledMilli) ||
    !integer(value.remainingMilli) ||
    value.fulfilledMilli > value.quantityMilli ||
    value.remainingMilli !== value.quantityMilli - value.fulfilledMilli ||
    !integer(value.priceMinor) ||
    !integer(value.taxBps) ||
    !integer(value.ordinal) ||
    (value.stock !== null && !object(value.stock)) ||
    !Array.isArray(value.lots) ||
    value.lots.length > 200 ||
    !boolean(value.lotsTruncated)
  ) {
    return fail('NATIVE_INVENTORY_RECEIPT_RESPONSE_INVALID');
  }
  const tracking = value.trackingMode as 'none' | 'lot' | 'serial';
  const lots = value.lots.map(receiptLot);
  if (
    lots.some((lot) =>
      tracking === 'lot'
        ? !lot.lot_code || !!lot.serial_number
        : tracking === 'serial'
          ? !lot.serial_number || !!lot.lot_code
          : true,
    )
  ) {
    return fail('NATIVE_INVENTORY_RECEIPT_RESPONSE_INVALID');
  }
  return {
    id: value.id,
    product_id: value.productId,
    sku: value.sku,
    description: value.description,
    unit: value.unit,
    tracking_mode: tracking,
    quantity_milli: value.quantityMilli,
    fulfilled_milli: value.fulfilledMilli,
    remaining_milli: value.remainingMilli,
    price_minor: value.priceMinor,
    tax_bps: value.taxBps,
    ordinal: value.ordinal,
    stock: value.stock === null ? null : receiptStock(value.stock),
    lots,
    lots_truncated: value.lotsTruncated,
  };
}

function validateReceipt(value: unknown, actor: NativeAvailabilityContext): NativeReceiptSource {
  if (
    !object(value) ||
    !contextMatches(value.context, actor) ||
    !object(value.data) ||
    value.data.companyId !== actor.companyId ||
    !object(value.data.order) ||
    !Array.isArray(value.data.lines) ||
    value.data.lines.length > 200
  ) {
    return fail('NATIVE_INVENTORY_RECEIPT_RESPONSE_INVALID');
  }
  const order = value.data.order;
  if (
    !text(order.id) ||
    !text(order.number) ||
    order.kind !== 'purchase' ||
    typeof order.state !== 'string' ||
    !integer(order.version, 1) ||
    !text(order.warehouseId) ||
    !text(order.branchId) ||
    !nullableText(order.departmentId) ||
    typeof order.orderDate !== 'string' ||
    !text(order.currency, 3) ||
    order.currency.length !== 3 ||
    typeof order.partyName !== 'string'
  ) {
    return fail('NATIVE_INVENTORY_RECEIPT_RESPONSE_INVALID');
  }
  const lines = value.data.lines.map(receiptLine);
  if (new Set(lines.map((line) => line.id)).size !== lines.length) {
    return fail('NATIVE_INVENTORY_RECEIPT_RESPONSE_INVALID');
  }
  const canReceive =
    order.kind === 'purchase' &&
    ['confirmed', 'partially_fulfilled'].includes(order.state) &&
    lines.some((line) => line.remaining_milli > 0);
  return {
    company_id: actor.companyId,
    order: {
      id: order.id,
      number: order.number,
      kind: order.kind,
      state: order.state,
      version: order.version,
      warehouse_id: order.warehouseId,
      branch_id: order.branchId,
      department_id: order.departmentId,
      order_date: order.orderDate,
      currency: order.currency,
      party_name: order.partyName,
    },
    lines,
    can_receive: canReceive,
  };
}

async function fetchJson(
  settings: Settings,
  incoming: Pick<Headers, 'get'>,
  slug: string,
  actor: NativeAvailabilityContext,
  path: string,
  transport: Transport,
  errorCode: string,
): Promise<unknown> {
  const host = incoming.get('host');
  const target = settings.holdings.find(
    (holding) => new URL(holding.origin).host === host && holding.holdingSlug === slug,
  );
  if (!target || target.holdingId !== actor.holdingId) {
    return fail(`${errorCode}_AUTHORITY_INVALID`);
  }
  const out = requestHeaders(incoming, actor);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), settings.timeoutMs);
  try {
    const url = new URL(path, settings.privateBaseUrl);
    const response = await transport(url, out, controller.signal);
    if (response.redirected || response.status !== 200) {
      void response.body?.cancel().catch(() => undefined);
      return fail(`${errorCode}_UNAVAILABLE`, response.status);
    }
    if (response.headers.get('content-type')?.split(';')[0] !== 'application/json') {
      return fail(`${errorCode}_RESPONSE_INVALID`);
    }
    return await response.json();
  } catch (error) {
    if (error instanceof NativeStockRegisterError) throw error;
    return fail(
      controller.signal.aborted
        ? `${errorCode}_ABORTED`
        : `${errorCode}_UNAVAILABLE`,
    );
  } finally {
    clearTimeout(timer);
  }
}

export function createNativeInventoryStockRegisterReader(
  raw: string,
  transport: Transport = direct,
) {
  const settings = configuration(raw);
  return async (
    incoming: Pick<Headers, 'get'>,
    slug: string,
    actor: NativeAvailabilityContext,
  ): Promise<NativeStockRegister> =>
    validateRegister(
      await fetchJson(
        settings,
        incoming,
        slug,
        actor,
        `/api/v1/holdings/${encodeURIComponent(slug)}/inventory/stock-register`,
        transport,
        'NATIVE_INVENTORY_STOCK',
      ),
      actor,
    );
}

export function createNativeInventoryReceiptReader(raw: string, transport: Transport = direct) {
  const settings = configuration(raw);
  return async (
    incoming: Pick<Headers, 'get'>,
    slug: string,
    actor: NativeAvailabilityContext,
    orderId: string,
  ): Promise<NativeReceiptSource> => {
    if (!orderId || orderId.length > 200 || [...orderId].some((value) => value < ' ')) {
      return fail('NATIVE_INVENTORY_RECEIPT_SELECTION_INVALID');
    }
    return validateReceipt(
      await fetchJson(
        settings,
        incoming,
        slug,
        actor,
        `/api/v1/holdings/${encodeURIComponent(slug)}/inventory/receipts/${encodeURIComponent(orderId)}`,
        transport,
        'NATIVE_INVENTORY_RECEIPT',
      ),
      actor,
    );
  };
}

export async function nativeInventoryStockRegisterFromHeaders(
  actor: NativeAvailabilityContext | undefined,
  holdingSlug: string,
): Promise<NativeStockRegister | undefined> {
  const raw = process.env.ERPX_NATIVE_INVENTORY_STOCK_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor) return fail('NATIVE_INVENTORY_STOCK_CONTEXT_CHANGED');
  return createNativeInventoryStockRegisterReader(raw)(await headers(), holdingSlug, actor);
}

export async function nativeInventoryReceiptFromHeaders(
  actor: NativeAvailabilityContext | undefined,
  holdingSlug: string,
  orderId: string,
): Promise<NativeReceiptSource | undefined> {
  const raw = process.env.ERPX_NATIVE_INVENTORY_STOCK_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor) return fail('NATIVE_INVENTORY_RECEIPT_CONTEXT_CHANGED');
  return createNativeInventoryReceiptReader(raw)(await headers(), holdingSlug, actor, orderId);
}
