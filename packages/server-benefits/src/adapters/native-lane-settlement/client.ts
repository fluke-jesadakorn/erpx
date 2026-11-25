import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { ApiError } from '@erpx/server-runtime/public/http';

/** Native settlement event-history transport. Carries no SQL, accepts no SQL
 * from a caller and has no TypeScript fallback. A missing, malformed or
 * failing native owner fails closed with ApiError(503). */
export const NATIVE_BENEFITS_SETTLEMENT_CONFIG = 'ERPX_NATIVE_BENEFITS_SETTLEMENT_CONFIG';
const RESERVED = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export type Config = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

export type SettlementEvent = Readonly<{ kind: string; image: string }>;

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

export function settlementConfiguration(raw: string | undefined): Config {
  if (!raw) return unavailable('BENEFIT_SETTLEMENT_EVENTS_READ_UNAVAILABLE', 'The native Benefits settlement reader is not configured.');
  let value: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!record(parsed)) throw new Error('not an object');
    value = parsed;
  } catch {
    return unavailable('BENEFIT_SETTLEMENT_EVENTS_READ_CONFIG_INVALID', 'The native Benefits settlement transport is misconfigured.');
  }
  if (
    !text(value.privateBaseUrl, 512) ||
    !text(value.host, 512) ||
    !text(value.holdingSlug, 128) ||
    !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !text(value.origin, 512) ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000
  )
    return unavailable('BENEFIT_SETTLEMENT_EVENTS_READ_CONFIG_INVALID', 'The native Benefits settlement transport is misconfigured.');
  let base: URL;
  let origin: URL;
  try {
    base = new URL(value.privateBaseUrl);
    origin = new URL(value.origin);
  } catch {
    return unavailable('BENEFIT_SETTLEMENT_EVENTS_READ_CONFIG_INVALID', 'The native Benefits settlement transport is misconfigured.');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl ||
    origin.protocol !== 'https:' ||
    origin.origin !== value.origin ||
    origin.host !== value.host ||
    origin.username ||
    origin.password
  )
    return unavailable('BENEFIT_SETTLEMENT_EVENTS_READ_CONFIG_INVALID', 'The native Benefits settlement transport is misconfigured.');
  return {
    base: base.origin,
    host: origin.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs,
  };
}

function httpGet(url: URL, headers: Record<string, string>, timeoutMs: number): Promise<{ status: number; value: unknown }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = httpRequest(
      url,
      { method: 'GET', agent, headers: { ...headers }, signal: AbortSignal.timeout(timeoutMs), maxHeaderSize: 8192 },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on('data', chunk => chunks.push(Buffer.from(chunk)));
        incoming.on('end', () => {
          agent.destroy();
          const body = Buffer.concat(chunks).toString('utf8');
          let value: unknown = null;
          try { value = body ? JSON.parse(body) : null; } catch { value = null; }
          resolve({ status: incoming.statusCode ?? 502, value });
        });
        incoming.on('error', error => { agent.destroy(); reject(error); });
      },
    );
    outgoing.on('error', error => { agent.destroy(); reject(error); });
    outgoing.end();
  });
}

function validEvent(value: unknown): value is SettlementEvent {
  return record(value)
    && Object.keys(value).length === 2
    && typeof value.kind === 'string'
    && value.kind.length <= 32
    && typeof value.image === 'string'
    && new TextEncoder().encode(value.image).length <= 2_000_000;
}

export function readSettlementEventsReply(value: unknown): SettlementEvent[] {
  if (
    !record(value) ||
    Object.keys(value).length !== 1 ||
    !Array.isArray(value.events) ||
    !value.events.every(validEvent)
  )
    return unavailable('BENEFIT_SETTLEMENT_EVENTS_READ_RESPONSE_INVALID', 'The native Benefits settlement reply was invalid.');
  return value.events as SettlementEvent[];
}

export function createSettlementEventsClient(cfg: Config, transport: Transport) {
  return async function read(companyId: string): Promise<SettlementEvent[]> {
    if (!/^[A-Za-z0-9_.:-]{1,200}$/.test(companyId))
      return unavailable('BENEFIT_SETTLEMENT_EVENTS_READ_INVALID', 'Use one bounded Benefits settlement event read.');
    let response: { status: number; value: unknown };
    try {
      response = await transport(
        new URL(
          `/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/benefits/settlement-events`,
          cfg.base,
        ),
        {
          host: cfg.host,
          origin: cfg.origin,
          accept: 'application/json',
          'cache-control': 'no-cache',
          'x-erpx-context-company': companyId,
        },
        cfg.timeoutMs,
      );
    } catch {
      return unavailable('BENEFIT_SETTLEMENT_EVENTS_READ_UNAVAILABLE', 'The native Benefits settlement reader is unavailable.');
    }
    if (response.status !== 200)
      return unavailable('BENEFIT_SETTLEMENT_EVENTS_READ_UNAVAILABLE', 'The native Benefits settlement reader is unavailable.');
    return readSettlementEventsReply(response.value);
  };
}

/** Native-backed settlement event history. The native route is the sole
 * authority; this transport issues no SQL and offers no local fallback. */
export async function benefitSettlementEvents(companyId: string): Promise<{ results: SettlementEvent[] }> {
  const cfg = settlementConfiguration(process.env[NATIVE_BENEFITS_SETTLEMENT_CONFIG]);
  return { results: await createSettlementEventsClient(cfg, httpGet)(companyId) };
}
