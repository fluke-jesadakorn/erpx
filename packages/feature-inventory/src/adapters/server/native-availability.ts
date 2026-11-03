import 'server-only';
import {Agent, request} from 'node:http';
import {headers} from 'next/headers';
import {allowedPrivateLoopbackPort} from '@erpx/native-sdk/loopback';

/** Owner-authorized context the page already resolved. */
export type NativeAvailabilityContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  revision: number;
  assignmentVersion: number;
  branchSlug?: string;
  departmentSlug?: string;
};

export type NativeAvailability = {
  rows: NativeAvailabilityRow[];
  currency: string;
};

export type NativeMovementRow = {
  id: string;
  warehouse_id: string;
  product_id: string;
  sku: string;
  warehouse_name: string;
  quantity_milli: number;
  value_minor: number;
  movement_date: string;
  source_type: string;
  source_id: string;
  order_id: string;
  order_number: string;
  fulfillment_date: string;
  created_at: number;
};

export type NativeMovements = {
  rows: NativeMovementRow[];
  nextCursor: string | null;
};

export type NativeAvailabilityRow = {
  warehouse_id: string;
  product_id: string;
  sku: string;
  name: string;
  unit: string;
  tracking_mode: string;
  warehouse_name: string;
  quantity_milli: number;
  reserved_milli: number;
  quarantined_milli: number;
  in_transit_milli: number;
  available_milli: number;
  value_minor: number;
  version: number;
};

type Settings = {
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: {origin: string; holdingSlug: string; holdingId: string}[];
  production?: boolean;
};
type Transport = (url: URL, headers: Headers, signal: AbortSignal) => Promise<Response>;

export class NativeAvailabilityError extends Error {
  constructor(
    readonly code: string,
    readonly status?: number,
  ) {
    super('The native Inventory availability read is unavailable.');
  }
}

const fail = (code: string, status?: number): never => {
  throw new NativeAvailabilityError(code, status);
};
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 200): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
function configuration(raw: string): Settings {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail('NATIVE_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID');
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
    return fail('NATIVE_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID');
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail('NATIVE_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID');
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
    return fail('NATIVE_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID');
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
      return fail('NATIVE_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID');
    }
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return fail('NATIVE_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID');
    }
    if (origin.origin !== holding.origin || origin.protocol !== 'https:') {
      return fail('NATIVE_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID');
    }
    if (hosts.has(origin.host) || slugs.has(holding.holdingSlug)) {
      return fail('NATIVE_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID');
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

function row(value: unknown): NativeAvailabilityRow {
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
    return fail('NATIVE_INVENTORY_AVAILABILITY_RESPONSE_INVALID');
  }
  const expected = Math.max(
    0,
    value.quantityMilli - Math.max(value.reservedMilli, value.quarantinedMilli),
  );
  if (value.availableMilli !== expected) {
    return fail('NATIVE_INVENTORY_AVAILABILITY_RESPONSE_INVALID');
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

function validate(
  value: unknown,
  actor: NativeAvailabilityContext,
): NativeAvailability {
  if (!object(value) || !object(value.context) || !Array.isArray(value.data)) {
    return fail('NATIVE_INVENTORY_AVAILABILITY_RESPONSE_INVALID');
  }
  const context = value.context;
  if (
    context.userId !== actor.userId ||
    context.holdingId !== actor.holdingId ||
    context.companyId !== actor.companyId ||
    context.branchId !== actor.branchId ||
    context.departmentId !== actor.departmentId ||
    context.assignmentId !== actor.assignmentId ||
    context.revision !== actor.revision ||
    context.assignmentVersion !== actor.assignmentVersion ||
    !integer(context.expiresAt, 1)
  ) {
    return fail('NATIVE_INVENTORY_AVAILABILITY_CONTEXT_CHANGED');
  }
  if (value.data.length > 1000 || !text(value.currency, 3) || value.currency.length !== 3) {
    return fail('NATIVE_INVENTORY_AVAILABILITY_RESPONSE_INVALID');
  }
  return {rows: value.data.map(row), currency: value.currency};
}

function movementRow(value: unknown): NativeMovementRow {
  if (
    !object(value) ||
    !text(value.id) ||
    !text(value.warehouseId) ||
    !text(value.productId) ||
    typeof value.sku !== 'string' ||
    typeof value.warehouseName !== 'string' ||
    typeof value.quantityMilli !== 'number' ||
    !Number.isSafeInteger(value.quantityMilli) ||
    typeof value.valueMinor !== 'number' ||
    !Number.isSafeInteger(value.valueMinor) ||
    typeof value.movementDate !== 'string' ||
    !text(value.sourceType) ||
    typeof value.sourceId !== 'string' ||
    typeof value.orderNumber !== 'string' ||
    typeof value.fulfillmentDate !== 'string' ||
    !integer(value.createdAt)
  ) {
    return fail('NATIVE_INVENTORY_MOVEMENT_RESPONSE_INVALID');
  }
  return {
    id: value.id,
    warehouse_id: value.warehouseId,
    product_id: value.productId,
    sku: value.sku,
    warehouse_name: value.warehouseName,
    quantity_milli: value.quantityMilli,
    value_minor: value.valueMinor,
    movement_date: value.movementDate,
    source_type: value.sourceType,
    source_id: value.sourceId,
    order_id: value.sourceId,
    order_number: value.orderNumber,
    fulfillment_date: value.fulfillmentDate,
    created_at: value.createdAt,
  };
}

function validateMovements(
  value: unknown,
  actor: NativeAvailabilityContext,
): NativeMovements {
  if (!object(value) || !object(value.context) || !Array.isArray(value.data)) {
    return fail('NATIVE_INVENTORY_MOVEMENT_RESPONSE_INVALID');
  }
  const context = value.context;
  if (
    context.userId !== actor.userId ||
    context.holdingId !== actor.holdingId ||
    context.companyId !== actor.companyId ||
    context.branchId !== actor.branchId ||
    context.departmentId !== actor.departmentId ||
    context.assignmentId !== actor.assignmentId ||
    context.revision !== actor.revision ||
    context.assignmentVersion !== actor.assignmentVersion ||
    !integer(context.expiresAt, 1)
  ) {
    return fail('NATIVE_INVENTORY_MOVEMENT_CONTEXT_CHANGED');
  }
  if (value.data.length > 50) {
    return fail('NATIVE_INVENTORY_MOVEMENT_RESPONSE_INVALID');
  }
  const next = value.nextCursor;
  if (next !== null && (typeof next !== 'string' || next.length === 0 || next.length > 400)) {
    return fail('NATIVE_INVENTORY_MOVEMENT_RESPONSE_INVALID');
  }
  return {rows: value.data.map(movementRow), nextCursor: next as string | null};
}

function requestHeaders(
  incoming: Pick<Headers, 'get'>,
  actor: NativeAvailabilityContext,
): {host: string; headers: Headers} | NativeAvailabilityError {
  const host = incoming.get('host');
  const out = new Headers({host: host!, accept: 'application/json', 'cache-control': 'no-cache'});
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192) {
      return new NativeAvailabilityError('NATIVE_INVENTORY_AVAILABILITY_COOKIE_LIMIT');
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
  return {host: host!, headers: out};
}

export function createNativeInventoryMovementsReader(raw: string, transport: Transport = direct) {
  const config = configuration(raw);
  return async (
    incoming: Pick<Headers, 'get'>,
    slug: string,
    actor: NativeAvailabilityContext,
    cursor?: string,
  ): Promise<NativeMovements> => {
    const host = incoming.get('host');
    const target = config.holdings.find(
      (holding) => new URL(holding.origin).host === host && holding.holdingSlug === slug,
    );
    if (!target || target.holdingId !== actor.holdingId) {
      return fail('NATIVE_INVENTORY_MOVEMENT_AUTHORITY_INVALID');
    }
    const built = requestHeaders(incoming, actor);
    if (built instanceof NativeAvailabilityError) throw built;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(slug)}/inventory/movements`,
        config.privateBaseUrl,
      );
      if (cursor !== undefined) {
        if (cursor.length === 0 || cursor.length > 400) {
          return fail('NATIVE_INVENTORY_MOVEMENT_CURSOR_INVALID');
        }
        url.searchParams.set('cursor', cursor);
      }
      const response = await transport(url, built.headers, controller.signal);
      if (response.redirected || response.status !== 200) {
        void response.body?.cancel().catch(() => undefined);
        return fail('NATIVE_INVENTORY_MOVEMENT_UNAVAILABLE', response.status);
      }
      if (response.headers.get('content-type')?.split(';')[0] !== 'application/json') {
        return fail('NATIVE_INVENTORY_MOVEMENT_RESPONSE_INVALID');
      }
      return validateMovements(await response.json(), actor);
    } catch (error) {
      if (error instanceof NativeAvailabilityError) throw error;
      return fail(
        controller.signal.aborted
          ? 'NATIVE_INVENTORY_MOVEMENT_ABORTED'
          : 'NATIVE_INVENTORY_MOVEMENT_UNAVAILABLE',
      );
    } finally {
      clearTimeout(timer);
    }
  };
}

export function createNativeInventoryAvailabilityReader(raw: string, transport: Transport = direct) {
  const config = configuration(raw);
  return async (
    incoming: Pick<Headers, 'get'>,
    slug: string,
    actor: NativeAvailabilityContext,
  ): Promise<NativeAvailability> => {
    const host = incoming.get('host');
    const target = config.holdings.find(
      (holding) => new URL(holding.origin).host === host && holding.holdingSlug === slug,
    );
    if (!target || target.holdingId !== actor.holdingId) {
      return fail('NATIVE_INVENTORY_AVAILABILITY_AUTHORITY_INVALID');
    }
    const out = new Headers({host: host!, accept: 'application/json', 'cache-control': 'no-cache'});
    const cookie = incoming.get('cookie');
    if (cookie) {
      if (new TextEncoder().encode(cookie).length > 8192) {
        return fail('NATIVE_INVENTORY_AVAILABILITY_COOKIE_LIMIT');
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
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(slug)}/inventory/availability`,
        config.privateBaseUrl,
      );
      const response = await transport(url, out, controller.signal);
      if (response.redirected || response.status !== 200) {
        void response.body?.cancel().catch(() => undefined);
        return fail('NATIVE_INVENTORY_AVAILABILITY_UNAVAILABLE', response.status);
      }
      if (response.headers.get('content-type')?.split(';')[0] !== 'application/json') {
        return fail('NATIVE_INVENTORY_AVAILABILITY_RESPONSE_INVALID');
      }
      return validate(await response.json(), actor);
    } catch (error) {
      if (error instanceof NativeAvailabilityError) throw error;
      return fail(
        controller.signal.aborted
          ? 'NATIVE_INVENTORY_AVAILABILITY_ABORTED'
          : 'NATIVE_INVENTORY_AVAILABILITY_UNAVAILABLE',
      );
    } finally {
      clearTimeout(timer);
    }
  };
}

export async function nativeInventoryAvailability(
  actor: NativeAvailabilityContext | undefined,
  incoming: Pick<Headers, 'get'>,
  holdingSlug: string,
): Promise<NativeAvailability | undefined> {
  const raw = process.env.ERPX_NATIVE_INVENTORY_AVAILABILITY_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor) return fail('NATIVE_INVENTORY_AVAILABILITY_CONTEXT_CHANGED');
  return createNativeInventoryAvailabilityReader(raw)(incoming, holdingSlug, actor);
}

export async function nativeInventoryAvailabilityFromHeaders(
  actor: NativeAvailabilityContext | undefined,
  holdingSlug: string,
): Promise<NativeAvailability | undefined> {
  return nativeInventoryAvailability(actor, await headers(), holdingSlug);
}

export async function nativeInventoryMovementsFromHeaders(
  actor: NativeAvailabilityContext | undefined,
  holdingSlug: string,
  cursor?: string,
): Promise<NativeMovements | undefined> {
  const raw = process.env.ERPX_NATIVE_INVENTORY_AVAILABILITY_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor) return fail('NATIVE_INVENTORY_MOVEMENT_CONTEXT_CHANGED');
  return createNativeInventoryMovementsReader(raw)(await headers(), holdingSlug, actor, cursor);
}
