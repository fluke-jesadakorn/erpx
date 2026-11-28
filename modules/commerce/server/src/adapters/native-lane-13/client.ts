import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

/**
 * Strict transport for the native Commerce entrypoint identity read family.
 *
 * The axum owner owns all five SQL statements, Company scoping, the projection
 * and the response shape. This bridge emits no SQL, accepts no SQL from a
 * caller and keeps no fallback reader: a missing, malformed or failing native
 * runtime fails closed with ApiError(503, ...).
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (code = 'COMMERCE_ENTRYPOINT_READ_UNAVAILABLE', message = 'The native Commerce entrypoint read service is unavailable.'): never => {
  throw new ApiError(503, code, message);
};

function config(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_COMMERCE_LANE13_CONFIG ?? process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
  if (raw === undefined) return unavailable('COMMERCE_ENTRYPOINT_READ_CONFIG_REQUIRED');
  let value: {
    privateBaseUrl?: unknown;
    host?: unknown;
    holdingSlug?: unknown;
    origin?: unknown;
    timeoutMs?: unknown;
  };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    return unavailable('COMMERCE_ENTRYPOINT_READ_CONFIG_INVALID');
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    return unavailable('COMMERCE_ENTRYPOINT_READ_CONFIG_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    origin.protocol !== 'https:' ||
    typeof value.host !== 'string' ||
    value.host !== origin.host ||
    typeof value.holdingSlug !== 'string' ||
    !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000
  )
    return unavailable('COMMERCE_ENTRYPOINT_READ_CONFIG_INVALID');
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === 'object' && !Array.isArray(v);
const text = (v: unknown, max = 200): v is string =>
  typeof v === 'string' && v.length > 0 && v.length <= max;

/** POST with an explicit Host header; fetch forbids overriding Host. */
function post(url: URL, headers: Record<string, string>, body: string, timeoutMs: number): Promise<{ status: number; value: unknown }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      {
        method: 'POST',
        agent,
        headers: { ...headers, 'content-length': String(Buffer.byteLength(body)) },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on('data', chunk => chunks.push(Buffer.from(chunk)));
        incoming.on('end', () => {
          agent.destroy();
          const raw = Buffer.concat(chunks).toString('utf8');
          let value: unknown = null;
          try {
            value = raw ? JSON.parse(raw) : null;
          } catch {
            value = null;
          }
          resolve({ status: incoming.statusCode ?? 502, value });
        });
        incoming.on('error', error => {
          agent.destroy();
          reject(error);
        });
      },
    );
    outgoing.on('error', error => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(body);
  });
}

export type EntrypointKind = 'orderKind' | 'orderFacts' | 'fulfillmentKind' | 'invoiceKind' | 'returnKind';
export type EntrypointIdentity = { kind: string };
export type EntrypointState = { kind: string; state: string };

/** One owner request; only a registered discriminator and a bounded id cross the wire. */
async function read(kind: EntrypointKind, id: string, companyId: string): Promise<Record<string, unknown> | null> {
  const cfg = config();
  if (!text(kind, 32) || !text(id, 200) || !text(companyId, 200)) return unavailable();
  let response: { status: number; value: unknown };
  try {
    response = await post(
      new URL(`/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/commerce/entrypoint-facts`, cfg.base),
      {
        host: cfg.host,
        origin: cfg.origin,
        accept: 'application/json',
        'content-type': 'application/json',
        'x-erpx-context-company': companyId,
      },
      JSON.stringify({ kind, id }),
      cfg.timeoutMs,
    );
  } catch {
    return unavailable();
  }
  const value: unknown = response.value;
  if (response.status !== 200) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(error.code, 100) ? String(error.code) : 'COMMERCE_ENTRYPOINT_READ_FAILED',
        text(error.message, 300) ? String(error.message) : 'The Commerce entrypoint read was rejected.',
      );
    return unavailable();
  }
  if (!isRecord(value) || !('facts' in value)) return unavailable('COMMERCE_ENTRYPOINT_READ_RESPONSE_INVALID');
  const facts = value.facts;
  if (facts === null || facts === undefined) return null;
  if (!isRecord(facts) || !text(facts.kind, 200)) return unavailable('COMMERCE_ENTRYPOINT_READ_RESPONSE_INVALID');
  return facts;
}

/** Kind-only entrypoint reader (retired `orderKind`/`fulfillmentKind`/`invoiceKind`/`returnKind`). */
export async function nativeCommerceEntrypointKind(kind: Exclude<EntrypointKind, 'orderFacts'>, id: string, companyId: string): Promise<EntrypointIdentity | null> {
  const facts = await read(kind, id, companyId);
  if (facts === null) return null;
  if ('state' in facts) return unavailable('COMMERCE_ENTRYPOINT_READ_RESPONSE_INVALID');
  return { kind: facts.kind as string };
}

/** Mutable order state reader (retired `orderFacts`). */
export async function nativeCommerceEntrypointFacts(id: string, companyId: string): Promise<EntrypointState | null> {
  const facts = await read('orderFacts', id, companyId);
  if (facts === null) return null;
  if (!text(facts.state, 200)) return unavailable('COMMERCE_ENTRYPOINT_READ_RESPONSE_INVALID');
  return { kind: facts.kind as string, state: facts.state };
}

/** True when the owner-native entrypoint transport is configured. */
export function nativeCommerceEntrypointConfigured(environment: Readonly<Record<string, string | undefined>> = process.env): boolean {
  return environment.ERPX_NATIVE_COMMERCE_LANE13_CONFIG !== undefined ||
    environment.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG !== undefined;
}

// ---- Strict multi-holding invoice/delivery client ------------------------
// The owner router is mounted once per registered Holding with one pool each.
// This bridge carries only a registered target identity, Company and bounded
// record id; it emits no SQL and keeps no fallback reader.

type InvoiceHolding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
}>;
type InvoiceConfig = Readonly<{
  base: string;
  timeoutMs: number;
  defaultHoldingSlug: string;
  holdings: readonly InvoiceHolding[];
}>;
export type InvoiceActor = Readonly<{
  companyId: string;
  holdingId?: string;
  holdingSlug?: string;
}>;
export type CommerceInvoiceSalesLine = Readonly<{
  id: string;
  fulfillment_id: string;
  description: string;
  quantity_milli: number;
  price_minor: number;
  tax_bps: number;
  subtotal_minor: number;
  tax_minor: number;
  item_type: string;
  product_id: string;
  completion_date: string;
}>;
export type CommerceCommandTarget = Readonly<{ kind: 'order'; id: string; action: 'invoice' }>;

const invoiceUnavailable = (code = 'COMMERCE_INVOICE_READ_UNAVAILABLE', message = 'The native Commerce invoice read service is unavailable.'): never => {
  throw new ApiError(503, code, message);
};

function invoiceConfig(): InvoiceConfig {
  const raw = process.env.ERPX_NATIVE_COMMERCE_LANE13_HOLDINGS_CONFIG;
  if (raw === undefined) return invoiceUnavailable('COMMERCE_INVOICE_READ_CONFIG_REQUIRED');
  let value: {
    privateBaseUrl?: unknown;
    timeoutMs?: unknown;
    defaultHoldingSlug?: unknown;
    holdings?: unknown;
  };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    return invoiceUnavailable('COMMERCE_INVOICE_READ_CONFIG_INVALID');
  }
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    return invoiceUnavailable('COMMERCE_INVOICE_READ_CONFIG_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== String(value.privateBaseUrl) ||
    !text(value.defaultHoldingSlug, 128) ||
    !/^[a-z0-9_-]+$/.test(value.defaultHoldingSlug) ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return invoiceUnavailable('COMMERCE_INVOICE_READ_CONFIG_INVALID');

  const holdings: InvoiceHolding[] = [];
  const seen = new Set<string>();
  const holdingIds = new Set<string>();
  for (const item of value.holdings) {
    if (
      !isRecord(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 253) ||
      !text(item.holdingId, 200) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(item.holdingSlug)
    )
      return invoiceUnavailable('COMMERCE_INVOICE_READ_CONFIG_INVALID');
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return invoiceUnavailable('COMMERCE_INVOICE_READ_CONFIG_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.host !== item.host ||
      origin.username ||
      origin.password
    )
      return invoiceUnavailable('COMMERCE_INVOICE_READ_CONFIG_INVALID');
    const key = `${item.host}|${item.holdingSlug}`;
    if (seen.has(key) || holdingIds.has(item.holdingId))
      return invoiceUnavailable('COMMERCE_INVOICE_READ_CONFIG_INVALID');
    seen.add(key);
    holdingIds.add(item.holdingId);
    holdings.push({
      origin: origin.origin,
      host: item.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
    });
  }
  if (!holdings.some(holding => holding.holdingSlug === value.defaultHoldingSlug))
    return invoiceUnavailable('COMMERCE_INVOICE_READ_CONFIG_INVALID');
  return {
    base: base.origin,
    timeoutMs: value.timeoutMs as number,
    defaultHoldingSlug: value.defaultHoldingSlug as string,
    holdings,
  };
}

function invoiceTarget(actor?: Pick<InvoiceActor, 'holdingId' | 'holdingSlug'>): InvoiceHolding {
  const cfg = invoiceConfig();
  if (actor && actor.holdingId !== undefined && actor.holdingSlug !== undefined) {
    const holding = cfg.holdings.find(
      item => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
    );
    if (!holding) throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The native Commerce Holding target changed.');
    return holding;
  }
  return cfg.holdings.find(item => item.holdingSlug === cfg.defaultHoldingSlug)!;
}

const num = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value);
const int = (value: unknown): value is number => num(value) && Number.isSafeInteger(value);

async function invoiceRequest(
  kind: 'deliveryTarget' | 'invoiceSalesLines',
  id: string,
  actor: InvoiceActor,
): Promise<Record<string, unknown>> {
  const cfg = invoiceConfig();
  const holding = invoiceTarget(actor);
  if (!text(kind, 32) || !text(id, 200) || !text(actor.companyId, 200))
    throw new ApiError(422, 'COMMERCE_INVOICE_READ_INPUT_INVALID', 'Use a bounded Commerce invoice read.');
  let response: { status: number; value: unknown };
  try {
    response = await post(
      new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/commerce/invoice-read`, cfg.base),
      {
        host: holding.host,
        origin: holding.origin,
        accept: 'application/json',
        'content-type': 'application/json',
        'x-erpx-context-company': actor.companyId,
      },
      JSON.stringify({ kind, id }),
      cfg.timeoutMs,
    );
  } catch {
    return invoiceUnavailable();
  }
  const value: unknown = response.value;
  if (response.status !== 200) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(error.code, 100) ? String(error.code) : 'COMMERCE_INVOICE_READ_FAILED',
        text(error.message, 300) ? String(error.message) : 'The Commerce invoice read was rejected.',
      );
    return invoiceUnavailable();
  }
  if (!isRecord(value)) return invoiceUnavailable('COMMERCE_INVOICE_READ_RESPONSE_INVALID');
  return value;
}

/** Retired `invoiceCommandFlowTarget`: Company-scoped delivery -> order identity. */
export async function nativeCommerceInvoiceTarget(
  deliveryId: string,
  actor: InvoiceActor,
): Promise<CommerceCommandTarget | null> {
  const value = await invoiceRequest('deliveryTarget', deliveryId, actor);
  if (!('target' in value) || 'lines' in value)
    return invoiceUnavailable('COMMERCE_INVOICE_READ_RESPONSE_INVALID');
  const target = value.target;
  if (target === null || target === undefined) return null;
  if (
    !isRecord(target) ||
    target.kind !== 'order' ||
    target.action !== 'invoice' ||
    !text(target.id, 200)
  )
    return invoiceUnavailable('COMMERCE_INVOICE_READ_RESPONSE_INVALID');
  return { kind: 'order', id: target.id as string, action: 'invoice' };
}

/** Retired `commerceInvoiceSalesLines`: delivered sales lines for a Company invoice. */
export async function nativeCommerceInvoiceSalesLines(
  invoiceId: string,
  actor: InvoiceActor,
): Promise<CommerceInvoiceSalesLine[]> {
  const value = await invoiceRequest('invoiceSalesLines', invoiceId, actor);
  if (!('lines' in value) || 'target' in value || !Array.isArray(value.lines))
    return invoiceUnavailable('COMMERCE_INVOICE_READ_RESPONSE_INVALID');
  return value.lines.map(line => {
    if (
      !isRecord(line) ||
      !text(line.id, 200) ||
      !text(line.fulfillment_id, 200) ||
      !text(line.description, 8192) ||
      !int(line.quantity_milli) ||
      !int(line.price_minor) ||
      !int(line.tax_bps) ||
      !int(line.subtotal_minor) ||
      !int(line.tax_minor) ||
      !text(line.item_type, 200) ||
      !text(line.product_id, 200) ||
      !text(line.completion_date, 200)
    )
      return invoiceUnavailable('COMMERCE_INVOICE_READ_RESPONSE_INVALID');
    return {
      id: line.id as string,
      fulfillment_id: line.fulfillment_id as string,
      description: line.description as string,
      quantity_milli: line.quantity_milli as number,
      price_minor: line.price_minor as number,
      tax_bps: line.tax_bps as number,
      subtotal_minor: line.subtotal_minor as number,
      tax_minor: line.tax_minor as number,
      item_type: line.item_type as string,
      product_id: line.product_id as string,
      completion_date: line.completion_date as string,
    };
  });
}

/** True when the strict multi-holding owner transport is configured. */
export function nativeCommerceInvoiceConfigured(environment: Readonly<Record<string, string | undefined>> = process.env): boolean {
  return environment.ERPX_NATIVE_COMMERCE_LANE13_HOLDINGS_CONFIG !== undefined;
}
