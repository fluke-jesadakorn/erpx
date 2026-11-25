import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { ApiError } from '@erpx/server-runtime/public/http';

/**
 * Strict owner transport for the native lane 38 Billing watermark read. The
 * owner crate executes the statement and projects the response; this bridge
 * carries no statement text, accepts none from a caller and has no TypeScript
 * fallback. A missing/misconfigured transport, a failed request or a malformed
 * reply fails closed with ApiError(503, ...).
 */
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;
type ConfigInput = Readonly<{
  privateBaseUrl?: unknown;
  timeoutMs?: unknown;
  holdings?: unknown;
}>;

/** Caller WorkContext projection used to select exactly one registered Holding. */
export type NativeLane38Context = Readonly<{
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;

export type NativeLane38Watermark = Readonly<{ table_name: string; revision: string }>;
export type NativeLane38Transport = (
  url: URL,
  headers: Record<string, string>,
  timeoutMs: number,
  method?: "GET" | "POST",
  body?: string,
) => Promise<{ status: number; value: unknown }>;

const WATERMARK_TABLE = 'billing_snapshot';
const ENV = 'ERPX_NATIVE_BILLING_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (code = 'NATIVE_LANE_38_BILLING_WATERMARK_UNAVAILABLE'): never => {
  throw new ApiError(503, code, 'The native Billing watermark service is unavailable.');
};
const invalid = (): never => {
  throw new ApiError(
    422,
    'NATIVE_LANE_38_BILLING_WATERMARK_REQUEST_INVALID',
    'Use one bounded Billing holding watermark read.',
  );
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const identifier = (value: unknown, max = 200): value is string =>
  text(value, max) && value.length > 0 && /^[A-Za-z0-9._:-]+$/.test(value);
const slug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]{1,128}$/.test(value);

function configuration(raw: unknown): Config {
  if (!raw || !text(raw, 16 * 1024)) return unavailable('NATIVE_LANE_38_BILLING_CONFIG_REQUIRED');
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable('NATIVE_LANE_38_BILLING_CONFIG_INVALID');
  }
  if (!record(value)) return unavailable('NATIVE_LANE_38_BILLING_CONFIG_INVALID');
  const input = value as ConfigInput;
  if (
    !text(input.privateBaseUrl, 512) ||
    typeof input.timeoutMs !== 'number' ||
    !Number.isSafeInteger(input.timeoutMs) ||
    input.timeoutMs < 250 ||
    input.timeoutMs > 30_000 ||
    !Array.isArray(input.holdings) ||
    input.holdings.length < 1 ||
    input.holdings.length > 64
  )
    return unavailable('NATIVE_LANE_38_BILLING_CONFIG_INVALID');
  let base: URL;
  try {
    base = new URL(input.privateBaseUrl);
  } catch {
    return unavailable('NATIVE_LANE_38_BILLING_CONFIG_INVALID');
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
    base.origin !== input.privateBaseUrl
  )
    return unavailable('NATIVE_LANE_38_BILLING_CONFIG_INVALID');
  const holdings: Holding[] = [];
  for (const item of input.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !identifier(item.holdingId) ||
      !slug(item.holdingSlug) ||
      (item.companyId !== undefined && !identifier(item.companyId))
    )
      return unavailable('NATIVE_LANE_38_BILLING_CONFIG_INVALID');
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable('NATIVE_LANE_38_BILLING_CONFIG_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.port ||
      origin.username ||
      origin.password ||
      origin.host !== item.host
    )
      return unavailable('NATIVE_LANE_38_BILLING_CONFIG_INVALID');
    holdings.push({
      origin: origin.origin,
      host: item.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
      ...(item.companyId === undefined ? {} : { companyId: item.companyId }),
    });
  }
  return { base: base.origin, timeoutMs: input.timeoutMs, holdings };
}

/** GET with an explicit Host header; fetch forbids overriding Host. */
const defaultTransport: NativeLane38Transport = (url, headers, timeoutMs, method = "GET", body) =>
  new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      {
        method,
        agent,
        headers: body === undefined ? headers : { ...headers, "content-type": "application/json" },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      (incoming) => {
        const chunks: Buffer[] = [];
        incoming.on('data', (chunk: Buffer) => chunks.push(chunk));
        incoming.on('end', () => {
          agent.destroy();
          const body = Buffer.concat(chunks).toString('utf8');
          let value: unknown = null;
          try {
            value = body ? JSON.parse(body) : null;
          } catch {
            value = null;
          }
          resolve({ status: incoming.statusCode ?? 502, value });
        });
        incoming.on('error', (error) => {
          agent.destroy();
          reject(error);
        });
      },
    );
    outgoing.on('error', (error) => {
      agent.destroy();
      reject(error);
    });
    if (body !== undefined) outgoing.write(body);
    outgoing.end();
  });

function validate(value: unknown): NativeLane38Watermark[] {
  if (!exact(value, ['watermarks']) || !Array.isArray(value.watermarks) || value.watermarks.length > 8)
    return unavailable('NATIVE_LANE_38_BILLING_WATERMARK_RESPONSE_INVALID');
  const out: NativeLane38Watermark[] = [];
  for (const item of value.watermarks) {
    if (
      !exact(item, ['table_name', 'revision']) ||
      item.table_name !== WATERMARK_TABLE ||
      !text(item.revision, 4096) ||
      item.revision.length === 0
    )
      return unavailable('NATIVE_LANE_38_BILLING_WATERMARK_RESPONSE_INVALID');
    out.push({ table_name: item.table_name, revision: item.revision });
  }
  return out;
}

// ---- Executive-main evidence family (wave 2) ------------------------------

const COMMERCIAL_KEYS = [
  'source_kind',
  'record_id',
  'plan_name',
  'status',
  'started_at',
  'effective_at',
  'record_version',
] as const;
const OVERDUE_KEYS = [
  'id',
  'kind',
  'status',
  'total_satang',
  'due_at',
  'updated_at',
  'month',
] as const;

export type NativeLane38CommercialRow = Readonly<{
  source_kind: "enrollment" | "legacy";
  record_id: string;
  plan_name: string;
  status: string;
  started_at: number | string | null;
  effective_at: number | string | null;
  record_version: number | string;
}>;
export type NativeLane38OverdueRow = Readonly<{
  id: string;
  kind: "main" | "adjustment";
  status: "open" | "uncollectible";
  total_satang: string | number;
  due_at: string | number;
  updated_at: string | number;
  month: string;
}>;
export type NativeLane38CommercialContext = { sourceKind: string; recordId: string };
export type NativeLane38CommercialSearch = {
  query: string;
  sourceKind?: string | null;
  recordId?: string | null;
  limit: number;
  now: number;
};
export type NativeLane38OverdueSearch = {
  from: string;
  to: string;
  query: string;
  dueAt?: number | null;
  id?: string | null;
  limit: number;
  end: number;
  now: number;
};

const unavailableEvidence = (): never => {
  throw new ApiError(
    503,
    'NATIVE_LANE_38_BILLING_EVIDENCE_UNAVAILABLE',
    'The native Billing evidence service is unavailable.',
  );
};
const numeric = (value: unknown): boolean =>
  typeof value === 'number' && Number.isSafeInteger(value) ||
  typeof value === 'string' && /^-?\d+$/.test(value) ||
  value === null;

function evidenceRows<T>(
  value: unknown,
  keys: readonly string[],
  shape: (row: Record<string, unknown>) => boolean,
  max: number,
): T[] {
  if (!exact(value, ['rows']) || !Array.isArray(value.rows) || value.rows.length > max)
    return unavailableEvidence();
  if (!value.rows.every((row: unknown) => exact(row, keys) && shape(row)))
    return unavailableEvidence();
  return value.rows as T[];
}

function evidenceRow<T>(
  value: unknown,
  keys: readonly string[],
  shape: (row: Record<string, unknown>) => boolean,
): T | null {
  if (!exact(value, ['row'])) return unavailableEvidence();
  const row = value.row;
  if (row === null) return null;
  if (!exact(row, keys) || !shape(row)) return unavailableEvidence();
  return row as T;
}

const commercialShape = (row: Record<string, unknown>): boolean =>
  text(row.source_kind, 40) &&
  ['enrollment', 'legacy'].includes(String(row.source_kind)) &&
  text(row.record_id, 200) &&
  text(row.plan_name, 200) &&
  text(row.status, 40) &&
  numeric(row.started_at) &&
  numeric(row.effective_at) &&
  numeric(row.record_version);

const overdueShape = (row: Record<string, unknown>): boolean =>
  text(row.id, 200) &&
  ['main', 'adjustment'].includes(String(row.kind)) &&
  ['open', 'uncollectible'].includes(String(row.status)) &&
  numeric(row.total_satang) &&
  numeric(row.due_at) &&
  numeric(row.updated_at) &&
  /^\d{4}-\d{2}$/.test(String(row.month));

/**
 * Build a native-backed watermark reader. The caller supplies its WorkContext
 * (holding id + slug); a bare holding id is accepted only for the retained
 * executive adapter and must resolve to exactly one configured Holding. There
 * is never a first-holding default and no statement text on this side.
 */
export function createNativeLane38Client(
  raw: string,
  transport: NativeLane38Transport = defaultTransport,
) {
  const config = configuration(raw);
  const select = (input: string | NativeLane38Context): { holding: Holding; company: string } => {
    let context: NativeLane38Context;
    if (typeof input === 'string') {
      const matches = config.holdings.filter((item) => item.holdingId === input);
      if (matches.length !== 1) return invalid();
      const holding = matches[0]!;
      context = {
        holdingId: holding.holdingId,
        holdingSlug: holding.holdingSlug,
        ...(holding.companyId === undefined ? {} : { companyId: holding.companyId }),
      };
    } else {
      context = input;
    }
    if (
      !record(context) ||
      !identifier(context.holdingId) ||
      !slug(context.holdingSlug) ||
      (context.companyId !== undefined && !identifier(context.companyId))
    )
      return invalid();
    const matches = config.holdings.filter(
      (item) =>
        item.holdingId === context.holdingId &&
        item.holdingSlug === context.holdingSlug &&
        (context.companyId === undefined || item.companyId === context.companyId),
    );
    if (matches.length !== 1) return invalid();
    const holding = matches[0]!;
    return {
      holding,
      company: context.companyId ?? holding.companyId ?? holding.holdingId,
    };
  };
  const post = async (
    context: string | NativeLane38Context,
    route: string,
    body: Record<string, unknown>,
  ): Promise<unknown> => {
    const { holding, company } = select(context);
    let response: { status: number; value: unknown };
    try {
      const encoded = JSON.stringify(body);
      if (new TextEncoder().encode(encoded).length > 16 * 1024) return invalid();
      response = await transport(
        new URL(
          `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/billing/native-lane-38/evidence/${route}`,
          config.base,
        ),
        {
          host: holding.host,
          origin: holding.origin,
          accept: 'application/json',
          'content-type': 'application/json',
          'cache-control': 'no-cache',
          'x-erpx-context-company': company,
        },
        config.timeoutMs,
        'POST',
        encoded,
      );
    } catch {
      return unavailableEvidence();
    }
    const value: unknown = response.value;
    if (response.status !== 200) {
      const error = record(value) && record(value.error) ? value.error : {};
      throw new ApiError(
        response.status >= 500 ? 503 : response.status,
        text(error.code, 200) ? String(error.code) : 'NATIVE_LANE_38_BILLING_EVIDENCE_FAILED',
        text(error.message, 2000) ? String(error.message) : 'The Billing evidence could not be read.',
      );
    }
    return value;
  };
  return {
    async watermarks(
      context: string | NativeLane38Context,
    ): Promise<NativeLane38Watermark[]> {
      const { holding, company } = select(context);
      let response: { status: number; value: unknown };
      try {
        response = await transport(
          new URL(
            `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/billing/native-lane-38/watermarks`,
            config.base,
          ),
          {
            host: holding.host,
            origin: holding.origin,
            accept: 'application/json',
            'cache-control': 'no-cache',
            'x-erpx-context-company': company,
          },
          config.timeoutMs,
        );
      } catch {
        return unavailable();
      }
      const value: unknown = response.value;
      if (response.status !== 200) {
        const error = record(value) && record(value.error) ? value.error : {};
        throw new ApiError(
          response.status >= 500 ? 503 : response.status,
          text(error.code, 200) ? String(error.code) : 'NATIVE_LANE_38_BILLING_WATERMARK_FAILED',
          text(error.message, 2000) ? String(error.message) : 'The Billing watermark could not be read.',
        );
      }
      return validate(value);
    },

    async commercialSubscriptions(
      context: string | NativeLane38Context,
      input: NativeLane38CommercialSearch,
    ): Promise<NativeLane38CommercialRow[]> {
      if (
        !record(input) ||
        typeof input.query !== 'string' ||
        input.query.length > 200 ||
        !Number.isSafeInteger(input.limit) ||
        input.limit < 1 ||
        input.limit > 100 ||
        !Number.isSafeInteger(input.now) ||
        input.now <= 0
      )
        return invalid();
      const body = {
        query: input.query,
        sourceKind: input.sourceKind ?? null,
        recordId: input.recordId ?? null,
        limit: input.limit,
        now: input.now,
      };
      return evidenceRows<NativeLane38CommercialRow>(
        await post(context, 'commercial-subscriptions', body),
        COMMERCIAL_KEYS,
        commercialShape,
        101,
      );
    },

    async commercialSubscription(
      context: string | NativeLane38Context,
      decoded: NativeLane38CommercialContext,
      now: number,
    ): Promise<NativeLane38CommercialRow | null> {
      if (
        !record(decoded) ||
        !['enrollment', 'legacy'].includes(String(decoded.sourceKind)) ||
        !text(decoded.recordId, 200) ||
        !Number.isSafeInteger(now) ||
        now <= 0
      )
        return invalid();
      return evidenceRow<NativeLane38CommercialRow>(
        await post(context, 'commercial-subscription', {
          sourceKind: decoded.sourceKind,
          recordId: decoded.recordId,
          now,
        }),
        COMMERCIAL_KEYS,
        commercialShape,
      );
    },

    async overdueInvoices(
      context: string | NativeLane38Context,
      input: NativeLane38OverdueSearch,
    ): Promise<NativeLane38OverdueRow[]> {
      const fromMs = Date.parse(input.from);
      if (
        !record(input) ||
        !/^\d{4}-\d{2}-\d{2}$/.test(input.from) ||
        !/^\d{4}-\d{2}-\d{2}$/.test(input.to) ||
        !Number.isSafeInteger(fromMs) ||
        typeof input.query !== 'string' ||
        input.query.length > 200 ||
        !Number.isSafeInteger(input.limit) ||
        input.limit < 1 ||
        input.limit > 100 ||
        !Number.isSafeInteger(input.end) ||
        input.end <= 0 ||
        !Number.isSafeInteger(input.now) ||
        input.now <= 0
      )
        return invalid();
      return evidenceRows<NativeLane38OverdueRow>(
        await post(context, 'overdue-invoices', {
          fromMs,
          query: input.query,
          dueAt: input.dueAt ?? null,
          id: input.id ?? null,
          limit: input.limit,
          end: input.end,
          now: input.now,
        }),
        OVERDUE_KEYS,
        overdueShape,
        101,
      );
    },

    async overdueInvoice(
      context: string | NativeLane38Context,
      filters: { from: string; to: string },
      now: number,
      id: string,
    ): Promise<NativeLane38OverdueRow | null> {
      const fromMs = Date.parse(filters.from);
      const toExclusiveMs = Date.parse(filters.to) + 86400000;
      if (
        !record(filters) ||
        !/^\d{4}-\d{2}-\d{2}$/.test(filters.from) ||
        !/^\d{4}-\d{2}-\d{2}$/.test(filters.to) ||
        !Number.isSafeInteger(fromMs) ||
        !Number.isSafeInteger(toExclusiveMs) ||
        !text(id, 200) ||
        !Number.isSafeInteger(now) ||
        now <= 0
      )
        return invalid();
      return evidenceRow<NativeLane38OverdueRow>(
        await post(context, 'overdue-invoice', { fromMs, toExclusiveMs, now, id }),
        OVERDUE_KEYS,
        overdueShape,
      );
    },
  };
}

/** Env-bound convenience reader used by the retained executive adapter. */
export async function nativeBillingWatermarks(
  context: string | NativeLane38Context,
): Promise<NativeLane38Watermark[]> {
  const raw = process.env[ENV];
  return createNativeLane38Client(raw ?? '', defaultTransport).watermarks(context);
}

const envClient = () => createNativeLane38Client(process.env[ENV] ?? '', defaultTransport);

export async function nativeBillingCommercialSubscriptions(
  context: string | NativeLane38Context,
  input: NativeLane38CommercialSearch,
): Promise<NativeLane38CommercialRow[]> {
  return envClient().commercialSubscriptions(context, input);
}

export async function nativeBillingCommercialSubscription(
  context: string | NativeLane38Context,
  decoded: NativeLane38CommercialContext,
  now: number,
): Promise<NativeLane38CommercialRow | null> {
  return envClient().commercialSubscription(context, decoded, now);
}

export async function nativeBillingOverdueInvoices(
  context: string | NativeLane38Context,
  input: NativeLane38OverdueSearch,
): Promise<NativeLane38OverdueRow[]> {
  return envClient().overdueInvoices(context, input);
}

export async function nativeBillingOverdueInvoice(
  context: string | NativeLane38Context,
  filters: { from: string; to: string },
  now: number,
  id: string,
): Promise<NativeLane38OverdueRow | null> {
  return envClient().overdueInvoice(context, filters, now, id);
}
