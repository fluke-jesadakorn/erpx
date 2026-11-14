import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import { allowedPrivateLoopbackPort } from '@erpx/contracts/private-loopback';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { AiProviderOverview } from '@erpx/server-agent/public/ai-providers';

/** Strict owner transport for the mounted native Agent AI-provider router. */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string }>;
export type NativeAgentAiProvidersConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Holding[];
  production?: boolean;
}>;

type Transport = (
  url: URL,
  method: 'GET' | 'POST',
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

const fail = (code: string, status = 503): never => {
  throw new ApiError(status, code, 'The native AI provider service is unavailable.');
};
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);

function configuration(raw: string): NativeAgentAiProvidersConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail('NATIVE_AGENT_AI_PROVIDERS_CONFIGURATION_INVALID');
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
    return fail('NATIVE_AGENT_AI_PROVIDERS_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail('NATIVE_AGENT_AI_PROVIDERS_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    !allowedPrivateLoopbackPort(port, value.production === true) ||
    base.origin !== value.privateBaseUrl ||
    base.username ||
    base.password
  )
    return fail('NATIVE_AGENT_AI_PROVIDERS_CONFIGURATION_INVALID');
  const hosts = new Set<string>(),
    slugs = new Set<string>();
  for (const holding of value.holdings) {
    if (
      !object(holding) ||
      typeof holding.origin !== 'string' ||
      typeof holding.holdingSlug !== 'string' ||
      typeof holding.holdingId !== 'string' ||
      !holding.holdingId ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug)
    )
      return fail('NATIVE_AGENT_AI_PROVIDERS_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return fail('NATIVE_AGENT_AI_PROVIDERS_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== holding.origin ||
      origin.username ||
      origin.password ||
      hosts.has(origin.host) ||
      slugs.has(holding.holdingSlug)
    )
      return fail('NATIVE_AGENT_AI_PROVIDERS_CONFIGURATION_INVALID');
    hosts.add(origin.host);
    slugs.add(holding.holdingSlug);
  }
  return value as NativeAgentAiProvidersConfiguration;
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
      proxyEnv: {} as import('node:http').ProxyEnv,
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
            reject(new Error('NATIVE_AGENT_AI_PROVIDERS_RESPONSE_LIMIT'));
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

export function nativeAgentAiProvidersAvailable(): boolean {
  return process.env.ERPX_NATIVE_AGENT_AI_PROVIDERS_CONFIG !== undefined;
}

export function createNativeAgentAiProviders(raw: string, transport: Transport = direct) {
  const settings = configuration(raw);
  const call = async (
    incoming: Pick<Headers, 'get'>,
    slug: string,
    path: string,
    method: 'GET' | 'POST',
    body?: string,
    key?: string,
  ): Promise<unknown> => {
    if (body !== undefined && body.length > 64 * 1024)
      return fail('NATIVE_AGENT_AI_PROVIDERS_BODY_LIMIT', 413);
    const host = incoming.get('host');
    const target = settings.holdings.find(
      holding => new URL(holding.origin).host === host && holding.holdingSlug === slug,
    );
    if (!target) return fail('NATIVE_AGENT_AI_PROVIDERS_AUTHORITY_INVALID');
    const out = new Headers({ host: host!, accept: 'application/json' });
    if (method === 'POST') {
      out.set('content-type', 'application/json');
      if (key) out.set('idempotency-key', key);
    }
    for (const name of [
      'cookie',
      'x-erpx-context-company',
      'x-erpx-context-assignment',
      'x-erpx-context-revision',
      'x-erpx-context-assignment-version',
      'x-erpx-workspace-branch',
      'x-erpx-workspace-department',
    ]) {
      const value = incoming.get(name);
      if (value) out.set(name, value);
    }
    // The native router requires the registered origin for mutations. The
    // retained route already rejected a foreign browser Origin, so a missing
    // header is the server-to-server case and uses the exact target origin.
    out.set('origin', incoming.get('origin') ?? target.origin);
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(slug)}/ai-providers${path}`,
      settings.privateBaseUrl,
    );
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), Number(settings.timeoutMs));
    try {
      const response = await transport(url, method, out, body, controller.signal);
      const payload = (await response.json().catch(() => null)) as
        | { data?: unknown; error?: { code?: string; message?: string } }
        | null;
      if (!response.ok)
        return fail(
          payload?.error?.code ?? 'NATIVE_AGENT_AI_PROVIDERS_UNAVAILABLE',
          response.status >= 400 && response.status <= 599 ? response.status : 503,
        );
      if (!object(payload) || !object(payload.data))
        return fail('NATIVE_AGENT_AI_PROVIDERS_RESPONSE_INVALID');
      return payload.data;
    } catch (error) {
      if (error instanceof ApiError) throw error;
      return fail(
        controller.signal.aborted
          ? 'NATIVE_AGENT_AI_PROVIDERS_ABORTED'
          : 'NATIVE_AGENT_AI_PROVIDERS_UNAVAILABLE',
      );
    } finally {
      clearTimeout(timer);
    }
  };
  return {
    overview: (incoming: Pick<Headers, 'get'>, slug: string) =>
      call(incoming, slug, '', 'GET') as Promise<AiProviderOverview>,
    models: (incoming: Pick<Headers, 'get'>, slug: string, id: string) =>
      call(incoming, slug, `/${encodeURIComponent(id)}/models`, 'GET'),
    command: (
      incoming: Pick<Headers, 'get'>,
      slug: string,
      path: string,
      body: string,
      key?: string,
    ) => call(incoming, slug, path, 'POST', body, key),
  };
}
