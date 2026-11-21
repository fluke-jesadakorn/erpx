import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';

/**
 * Strict server-side client for the native sandbox email delivery axum route.
 * This is the only write path for `sandbox_email_deliveries`; it carries no SQL
 * and offers no fallback. Encryption of sensitive bodies stays in the caller;
 * this client forwards only bounded, already-encrypted facts to the owner unit.
 */
export type NativeEmailDeliveryConfig = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

export type NativeEmailDeliveryInput = Readonly<{
  id: string;
  deliveryKey: string;
  recipient: string;
  subject: string;
  bodyText: string;
  bodyCiphertext: string | null;
  providerId: string;
  createdAt: number;
  metadataJson: string;
}>;

export class NativeEmailDeliveryError extends Error {
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native sandbox email delivery command could not be completed.');
    this.name = 'NativeEmailDeliveryError';
    this.status = status;
  }
}

const fail = (status?: number): never => {
  throw new NativeEmailDeliveryError('NATIVE_MANAGE_ERPX_UNAVAILABLE', status);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const identifier = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= 200;
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: string | undefined): NativeEmailDeliveryConfig {
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings))
    return fail();
  const timeoutMs = value.timeoutMs;
  if (typeof timeoutMs !== 'number' || !Number.isSafeInteger(timeoutMs) || timeoutMs < 250 || timeoutMs > 30000)
    return fail();
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail();
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reservedPorts.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username || base.password || value.privateBaseUrl !== base.origin
  )
    return fail();
  const holdings = value.holdings.map((item: unknown) => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string' || typeof item.holdingId !== 'string')
      return fail();
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return fail();
    }
    if (
      origin.protocol !== 'https:' ||
      item.origin !== origin.origin ||
      origin.username || origin.password ||
      !identifier(item.holdingId) || !/^[a-z0-9_-]+$/.test(item.holdingSlug)
    )
      return fail();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId };
  });
  if (holdings.length < 1 || holdings.length > 64) return fail();
  return { privateBaseUrl: value.privateBaseUrl, holdings, timeoutMs };
}

function directHttp(
  url: URL,
  headers: Record<string, string>,
  body: string,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = httpRequest(
      url,
      { method: 'POST', agent, headers: { ...headers, 'content-length': String(new TextEncoder().encode(body).length) }, signal, maxHeaderSize: 8192 },
      incoming => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          if (status !== 200) {
            incoming.destroy();
            resolve(new Response(null, { status: status >= 200 && status <= 599 ? status : 502 }));
            return;
          }
          const values = new Headers();
          for (let i = 0; i < incoming.rawHeaders.length; i += 2) values.append(incoming.rawHeaders[i]!, incoming.rawHeaders[i + 1]!);
          resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, { status, headers: values }));
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', error => { agent.destroy(); reject(error); });
    outgoing.write(body);
    outgoing.end();
  });
}

export function nativeEmailDeliveryAvailable(): boolean {
  return process.env.ERPX_NATIVE_MANAGE_ERPX_CONFIG !== undefined;
}

/** Reserve one sandbox email delivery through the native axum command. No SQL
 * and no fallback: a missing transport or invalid reply fails closed. */
export async function nativeReserveEmailDelivery(
  input: NativeEmailDeliveryInput,
  key: string,
): Promise<void> {
  const config = configuration(process.env.ERPX_NATIVE_MANAGE_ERPX_CONFIG);
  if (!/^[A-Za-z0-9:_.-]{8,180}$/.test(key)) return fail();
  if (
    !identifier(input.id) || !identifier(input.deliveryKey) ||
    typeof input.recipient !== 'string' || input.recipient.length === 0 || input.recipient.length > 320 ||
    typeof input.subject !== 'string' || input.subject.length > 4096 ||
    typeof input.bodyText !== 'string' || input.bodyText.length > 131072 ||
    (input.bodyCiphertext !== null && (typeof input.bodyCiphertext !== 'string' || !input.bodyCiphertext.startsWith('v1.'))) ||
    !identifier(input.providerId) || !Number.isSafeInteger(input.createdAt) || input.createdAt <= 0 ||
    typeof input.metadataJson !== 'string' || !input.metadataJson.startsWith('{')
  )
    return fail();
  const holding = config.holdings.find(item => item.holdingId === key) ?? config.holdings[0]!;
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/manage/sandbox-email/delivery`,
    config.privateBaseUrl,
  );
  const body = JSON.stringify(input);
  if (new TextEncoder().encode(body).length > 192 * 1024) return fail();
  const headers: Record<string, string> = {
    host: new URL(holding.origin).host,
    origin: holding.origin,
    'idempotency-key': key,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
  };
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await directHttp(url, headers, body, controller.signal);
    if (response.redirected || response.status !== 200 || !response.body) return fail(response.status);
    const text = await response.text();
    if (text.length > 16 * 1024) return fail();
    const payload = JSON.parse(text) as { data?: unknown };
    if (!record(payload) || !record(payload.data) || payload.data.deliveryKey !== input.deliveryKey) return fail();
  } catch (error) {
    if (error instanceof NativeEmailDeliveryError) throw error;
    return fail();
  } finally {
    clearTimeout(timer);
  }
}
