import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

/**
 * Native lane-32 Workflow transport for the failed inbox metric. This is the
 * sole successor of the `outbox_events` count in
 * `adapters/collaboration-inbox-sources.ts`: it carries no SQL, accepts no
 * caller predicate and has no TypeScript fallback. Missing configuration, an
 * unregistered Holding or any failed reply fails closed with ApiError(503).
 */
export const NATIVE_WORKFLOW_LANE32_CONFIG = 'ERPX_NATIVE_WORKFLOW_LANE32_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
}>;

export type NativeWorkflowLane32Config = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;

/**
 * Caller-supplied scope. A WorkContext is the pair (holdingId, holdingSlug);
 * without one, an explicit registered `origin` is required. There is no
 * positional or first-Holding fallback.
 */
export type HoldingSelector = Readonly<{
  holdingId?: string;
  holdingSlug?: string;
  origin?: string;
}>;

type Transport = (
  url: URL,
  headers: Record<string, string>,
  timeoutMs: number,
) => Promise<{ status: number; value: unknown }>;

const unavailable = (code: string, message: string): never => {
  throw new ApiError(503, code, message);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max: number): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;

/** Strict private, multi-Holding configuration. Only loopback, non-reserved ports are trusted. */
export function nativeWorkflowLane32Configuration(
  raw: string | undefined,
): NativeWorkflowLane32Config {
  if (!raw) return unavailable('WORKFLOW_FAILED_METRICS_READ_UNAVAILABLE', 'The native Workflow failed-metrics service is not configured.');
  let value: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!record(parsed)) throw new Error('not an object');
    value = parsed;
  } catch {
    return unavailable('WORKFLOW_FAILED_METRICS_READ_CONFIG_INVALID', 'The native Workflow failed-metrics transport is misconfigured.');
  }
  if (
    !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable('WORKFLOW_FAILED_METRICS_READ_CONFIG_INVALID', 'The native Workflow failed-metrics transport is misconfigured.');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable('WORKFLOW_FAILED_METRICS_READ_CONFIG_INVALID', 'The native Workflow failed-metrics transport is misconfigured.');
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
    base.origin !== value.privateBaseUrl
  )
    return unavailable('WORKFLOW_FAILED_METRICS_READ_CONFIG_INVALID', 'The native Workflow failed-metrics transport is misconfigured.');
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !text(item.holdingId, 200) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug)
    )
      return unavailable('WORKFLOW_FAILED_METRICS_READ_CONFIG_INVALID', 'The native Workflow failed-metrics transport is misconfigured.');
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable('WORKFLOW_FAILED_METRICS_READ_CONFIG_INVALID', 'The native Workflow failed-metrics transport is misconfigured.');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.host !== item.host ||
      origin.username ||
      origin.password
    )
      return unavailable('WORKFLOW_FAILED_METRICS_READ_CONFIG_INVALID', 'The native Workflow failed-metrics transport is misconfigured.');
    if (
      holdings.some(
        held =>
          held.holdingId === item.holdingId ||
          held.holdingSlug === item.holdingSlug ||
          held.host === item.host,
      )
    )
      return unavailable('WORKFLOW_FAILED_METRICS_READ_CONFIG_INVALID', 'The native Workflow failed-metrics transport is misconfigured.');
    holdings.push({
      origin: origin.origin,
      host: origin.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** GET with an explicit Host header; `fetch` forbids overriding Host. */
function httpGet(
  url: URL,
  headers: Record<string, string>,
  timeoutMs: number,
): Promise<{ status: number; value: unknown }> {
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
        method: 'GET',
        agent,
        headers: { ...headers },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on('data', chunk => chunks.push(Buffer.from(chunk)));
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
    outgoing.end();
  });
}

/** Strictly validate the native reply; any deviation fails closed. */
export function readFailedMetricsReply(value: unknown): { count: number } {
  if (
    !record(value) ||
    Object.keys(value).length !== 1 ||
    !Number.isSafeInteger(value.count) ||
    (value.count as number) < 0
  )
    return unavailable('WORKFLOW_FAILED_METRICS_READ_RESPONSE_INVALID', 'The native Workflow failed-metrics reply was invalid.');
  return { count: value.count as number };
}

/** Select one configured Holding from the caller's WorkContext or an explicit
 * registered origin. No positional fallback: an absent or partial scope fails
 * closed. */
export function selectHolding(
  cfg: NativeWorkflowLane32Config,
  selector?: HoldingSelector,
): Holding {
  const fail = () =>
    unavailable('WORKFLOW_FAILED_METRICS_AUTHORITY_INVALID', 'The native Workflow WorkContext is unavailable.');
  if (!selector) return fail();
  const fromWorkContext =
    selector.holdingId !== undefined || selector.holdingSlug !== undefined;
  let found: Holding | undefined;
  if (fromWorkContext) {
    if (
      typeof selector.holdingId !== 'string' ||
      selector.holdingId.length < 1 ||
      selector.holdingId.length > 200 ||
      typeof selector.holdingSlug !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(selector.holdingSlug)
    )
      return fail();
    found = cfg.holdings.find(
      holding =>
        holding.holdingId === selector.holdingId &&
        holding.holdingSlug === selector.holdingSlug,
    );
  } else if (typeof selector.origin === 'string') {
    found = cfg.holdings.find(holding => holding.origin === selector.origin);
  }
  if (!found) return fail();
  return found;
}

/** Injectable multi-Holding client used by the contract test and the adapter. */
export function createFailedMetricsClient(cfg: NativeWorkflowLane32Config, transport: Transport) {
  return async function read(selector?: HoldingSelector): Promise<{ count: number }> {
    const holding = selectHolding(cfg, selector);
    let response: { status: number; value: unknown };
    try {
      response = await transport(
        new URL(
          `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/workflow/inbox-failed-metrics`,
          cfg.base,
        ),
        {
          host: holding.host,
          origin: holding.origin,
          accept: 'application/json',
          'cache-control': 'no-cache',
        },
        cfg.timeoutMs,
      );
    } catch {
      return unavailable('WORKFLOW_FAILED_METRICS_READ_UNAVAILABLE', 'The native Workflow failed-metrics service is unavailable.');
    }
    if (response.status !== 200)
      return unavailable('WORKFLOW_FAILED_METRICS_READ_UNAVAILABLE', 'The native Workflow failed-metrics service is unavailable.');
    return readFailedMetricsReply(response.value);
  };
}

/**
 * Native-backed failed inbox metric. `database` is retained for call-site
 * compatibility only; no SQL is issued on it.
 */
export async function nativeInboxFailedMetrics(
  _database: SqlDatabase,
  selector?: HoldingSelector,
): Promise<{ count: number }> {
  const cfg = nativeWorkflowLane32Configuration(process.env[NATIVE_WORKFLOW_LANE32_CONFIG]);
  return createFailedMetricsClient(cfg, httpGet)(selector);
}
