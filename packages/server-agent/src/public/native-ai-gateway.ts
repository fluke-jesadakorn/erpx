import 'server-only';
import { Agent, request, type ProxyEnv } from 'node:http';
import { Readable } from 'node:stream';
import { ApiError } from '@erpx/server-runtime/public/http';

/**
 * Strict owner transport for the mounted native Platform AI-gateway router.
 * The route fails closed without `ERPX_NATIVE_AGENT_AI_GATEWAY_CONFIG`; there
 * is no TypeScript policy/statement/cost fallback.
 */
export type NativeAgentAiGatewayPolicy = Readonly<{
  platformEnabled: boolean;
  monthlyRequestLimit: number;
  monthlySpendCapSatang: number;
  reservationSatang: number;
  monthlyRequests: number;
  monthlyReservedOrVerifiedSatang: number;
  pendingPlatformCostRequests: number;
  updatedBy: string;
  updatedAt: number;
}>;

type Holding = Readonly<{ origin: string; holdingId: string }>;

export type NativeAgentAiGatewayConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  production?: boolean;
  holdings: readonly Holding[];
}>;

type Transport = (
  url: URL,
  method: 'GET' | 'POST',
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

// Mirrors `allowedPrivateLoopbackPort` without adding a package dependency.
const reservedPorts = new Set([3000, 3001, 3100, 3103, 5432, 5433, 5435, 8080]);
const loopback = (port: number, production: boolean) =>
  Number.isInteger(port) &&
  port >= 1 &&
  port <= 65535 &&
  !(port >= 3200 && port <= 3299) &&
  !reservedPorts.has(port) &&
  (production || port !== 3101);

const fail = (code: string, status = 503): never => {
  throw new ApiError(status, code, 'The native AI-gateway service is unavailable.');
};
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  object(value) && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;

function policy(value: unknown): NativeAgentAiGatewayPolicy {
  if (
    !exact(value, [
      'platformEnabled',
      'monthlyRequestLimit',
      'monthlySpendCapSatang',
      'reservationSatang',
      'monthlyRequests',
      'monthlyReservedOrVerifiedSatang',
      'pendingPlatformCostRequests',
      'updatedBy',
      'updatedAt',
    ]) ||
    typeof value.platformEnabled !== 'boolean' ||
    !integer(value.monthlyRequestLimit, 1) ||
    !integer(value.monthlySpendCapSatang, 1) ||
    !integer(value.reservationSatang, 1) ||
    !integer(value.monthlyRequests) ||
    !integer(value.monthlyReservedOrVerifiedSatang) ||
    !integer(value.pendingPlatformCostRequests) ||
    typeof value.updatedBy !== 'string' ||
    !value.updatedBy ||
    Buffer.byteLength(value.updatedBy) > 200 ||
    !integer(value.updatedAt)
  )
    return fail('NATIVE_AGENT_AI_GATEWAY_RESPONSE_INVALID');
  return value as NativeAgentAiGatewayPolicy;
}

const ok = (value: unknown): { ok: true } => {
  if (!exact(value, ['ok']) || value.ok !== true)
    return fail('NATIVE_AGENT_AI_GATEWAY_RESPONSE_INVALID');
  return { ok: true };
};

function configuration(raw: string): NativeAgentAiGatewayConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail('NATIVE_AGENT_AI_GATEWAY_CONFIGURATION_INVALID');
  }
  if (
    !object(value) ||
    (value.production !== undefined && typeof value.production !== 'boolean') ||
    typeof value.privateBaseUrl !== 'string' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    Number(value.timeoutMs) < 250 ||
    Number(value.timeoutMs) > 30_000 ||
    !Array.isArray(value.holdings) ||
    !value.holdings.length ||
    value.holdings.length > 64
  )
    return fail('NATIVE_AGENT_AI_GATEWAY_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail('NATIVE_AGENT_AI_GATEWAY_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    !loopback(port, value.production === true) ||
    base.origin !== value.privateBaseUrl ||
    base.username ||
    base.password
  )
    return fail('NATIVE_AGENT_AI_GATEWAY_CONFIGURATION_INVALID');
  const hosts = new Set<string>(),
    ids = new Set<string>();
  for (const holding of value.holdings) {
    if (
      !object(holding) ||
      typeof holding.origin !== 'string' ||
      typeof holding.holdingId !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingId)
    )
      return fail('NATIVE_AGENT_AI_GATEWAY_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return fail('NATIVE_AGENT_AI_GATEWAY_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== holding.origin ||
      origin.username ||
      origin.password ||
      hosts.has(origin.host) ||
      ids.has(holding.holdingId)
    )
      return fail('NATIVE_AGENT_AI_GATEWAY_CONFIGURATION_INVALID');
    hosts.add(origin.host);
    ids.add(holding.holdingId);
  }
  return value as NativeAgentAiGatewayConfiguration;
}

function direct(
  url: URL,
  method: 'GET' | 'POST',
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as ProxyEnv,
    });
    const req = request(
      url,
      { method, agent, headers: Object.fromEntries(headers), signal, maxHeaderSize: 8192 },
      incoming => {
        const chunks: Uint8Array[] = [];
        let total = 0;
        incoming.on('data', (chunk: Uint8Array) => {
          total += chunk.byteLength;
          if (total > 1024 * 1024) {
            incoming.destroy();
            reject(new Error('NATIVE_AGENT_AI_GATEWAY_RESPONSE_LIMIT'));
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
    if (body === undefined) req.end();
    else Readable.from([body]).pipe(req);
  });
}

export function nativeAgentAiGatewayAvailable(): boolean {
  return process.env.ERPX_NATIVE_AGENT_AI_GATEWAY_CONFIG !== undefined;
}

export function createNativeAgentAiGateway(raw: string, transport: Transport = direct) {
  const settings = configuration(raw);
  const call = async (
    incoming: Pick<Headers, 'get'>,
    method: 'GET' | 'POST',
    path: string,
    holdingId: string,
    body?: Record<string, unknown>,
  ): Promise<unknown> => {
    const target = settings.holdings.find(
      holding => new URL(holding.origin).host === incoming.get('host'),
    );
    if (!target) return fail('NATIVE_AGENT_AI_GATEWAY_AUTHORITY_INVALID');
    if (!/^[a-z0-9_-]{1,128}$/.test(holdingId))
      return fail('NATIVE_AGENT_AI_GATEWAY_AUTHORITY_INVALID', 422);
    const payload = body === undefined ? undefined : JSON.stringify(body);
    if (payload !== undefined && Buffer.byteLength(payload) > 64 * 1024)
      return fail('NATIVE_AGENT_AI_GATEWAY_BODY_LIMIT', 413);
    const headers = new Headers({ host: new URL(target.origin).host, accept: 'application/json' });
    if (payload !== undefined) headers.set('content-type', 'application/json');
    for (const name of [
      'cookie',
      'x-erpx-platform-assignment',
      'x-erpx-context-company',
      'x-erpx-context-assignment',
      'x-erpx-context-revision',
      'x-erpx-context-assignment-version',
      'x-erpx-workspace-branch',
      'x-erpx-workspace-department',
    ]) {
      const value = incoming.get(name);
      if (value) headers.set(name, value);
    }
    // The router requires the exact registered origin for mutations; the
    // retained route already rejected a foreign browser Origin, so a missing
    // header is the server-to-server case and uses the target origin.
    headers.set('origin', incoming.get('origin') ?? target.origin);
    const url = new URL(
      `/api/v1/platform/billing/ai-gateway${path}`,
      settings.privateBaseUrl,
    );
    if (method === 'GET') url.searchParams.set('holdingId', holdingId);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), Number(settings.timeoutMs));
    try {
      const response = await transport(url, method, headers, payload, controller.signal);
      const parsed = (await response.json().catch(() => null)) as
        | { data?: unknown; error?: { code?: string; message?: string } }
        | null;
      if (!response.ok) {
        const code = parsed?.error?.code;
        const message = parsed?.error?.message;
        throw new ApiError(
          response.status >= 400 && response.status <= 599 ? response.status : 503,
          typeof code === 'string' && code ? code : 'NATIVE_AGENT_AI_GATEWAY_FAILED',
          typeof message === 'string' && message
            ? message
            : 'The AI-gateway request failed.',
        );
      }
      if (!object(parsed) || !('data' in parsed))
        return fail('NATIVE_AGENT_AI_GATEWAY_RESPONSE_INVALID');
      return parsed.data;
    } catch (error) {
      if (error instanceof ApiError) throw error;
      return fail(
        controller.signal.aborted
          ? 'NATIVE_AGENT_AI_GATEWAY_ABORTED'
          : 'NATIVE_AGENT_AI_GATEWAY_UNAVAILABLE',
      );
    } finally {
      clearTimeout(timer);
    }
  };
  return {
    policy: async (incoming: Pick<Headers, 'get'>, holdingId: string) =>
      policy(await call(incoming, 'GET', '', holdingId)),
    setEnabled: async (
      incoming: Pick<Headers, 'get'>,
      body: Record<string, unknown>,
    ): Promise<NativeAgentAiGatewayPolicy> => {
      const holdingId = String(body.holdingId);
      return policy(await call(incoming, 'POST', '', holdingId, body));
    },
    updateLimits: async (incoming: Pick<Headers, 'get'>, body: Record<string, unknown>) =>
      policy(await call(incoming, 'POST', '/limits', String(body.holdingId), body)),
    recordStatement: async (
      incoming: Pick<Headers, 'get'>,
      body: Record<string, unknown>,
    ) => ok(await call(incoming, 'POST', '/statements', String(body.holdingId), body)),
    recordCost: async (incoming: Pick<Headers, 'get'>, body: Record<string, unknown>) =>
      ok(await call(incoming, 'POST', '/cost-evidence', String(body.holdingId), body)),
    reconcile: async (incoming: Pick<Headers, 'get'>, body: Record<string, unknown>) =>
      ok(await call(incoming, 'POST', '/reconcile', String(body.holdingId), body)),
  };
}
