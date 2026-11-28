import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';

/**
 * Strict private transport for the native field-command routing read. The
 * mounted axum route
 * `/api/v1/holdings/<holding>/collaboration/native-lane-15/field-command` is the
 * sole authority for both `collaboration_*` SELECTs and the boolean projection.
 * This bridge emits no SQL and accepts no SQL from a caller.
 *
 * The target is selected by the caller's WorkContext `holdingId` + `holdingSlug`
 * over 1..64 registered holdings; there is no default holding and any unknown,
 * ambiguous or mismatched target fails closed. Missing configuration, rejected
 * requests and malformed replies fail closed with ApiError(503, ...).
 */
type Row = Record<string, unknown>;
type Holding = {
  origin: string; host: string; holdingSlug: string; holdingId: string; companyId?: string;
};
type Settings = { privateBaseUrl: string; timeoutMs: number; holdings: Holding[] };
type FieldActor = Pick<WorkContext, 'holdingId' | 'holdingSlug' | 'companyId'>;

export const FIELD_COMMAND_CONFIG_ENV = 'ERPX_NATIVE_COLLABORATION_LANE15_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const fail = (status: number, code: string, message = 'The native field-command routing read is unavailable.'): never => {
  throw new ApiError(status, code, message);
};
const unavailable = (): never => fail(503, 'NATIVE_COLLABORATION_LANE15_UNAVAILABLE');
const record = (value: unknown): value is Row => value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && /^[A-Za-z0-9._:-]+$/.test(value);
const slug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]+$/.test(value);

function settings(raw: string): Settings {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID'); }
  if (!record(value) || !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 || value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) || value.holdings.length < 1 || value.holdings.length > 64)
    return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID');
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID'); }
  const port = Number(base.port);
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port || RESERVED_PORTS.has(port) || (port >= 3200 && port <= 3299) ||
    base.username || base.password || base.origin !== value.privateBaseUrl)
    return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID');
  const hosts = new Set<string>();
  const ids = new Set<string>();
  const slugs = new Set<string>();
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (!record(item) || !text(item.origin, 512) || !text(item.host, 512) ||
      !identifier(item.holdingId) || !slug(item.holdingSlug) ||
      (item.companyId !== undefined && !identifier(item.companyId)))
      return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID');
    let origin: URL;
    try { origin = new URL(String(item.origin)); } catch { return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID'); }
    if (origin.protocol !== 'https:' || origin.origin !== item.origin || origin.username ||
      origin.password || origin.host !== item.host ||
      hosts.has(String(item.host)) || ids.has(String(item.holdingId)) ||
      slugs.has(String(item.holdingSlug)))
      return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID');
    hosts.add(String(item.host));
    ids.add(String(item.holdingId));
    slugs.add(String(item.holdingSlug));
    holdings.push({
      origin: origin.origin, host: String(item.host), holdingSlug: String(item.holdingSlug),
      holdingId: String(item.holdingId),
      ...(item.companyId !== undefined ? { companyId: String(item.companyId) } : {}),
    });
  }
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Exact WorkContext identity only; no first or default holding. */
function target(config: Settings, actor: FieldActor): Holding {
  if (!record(actor as unknown as Row) || !identifier(actor.holdingId) ||
    !slug(actor.holdingSlug) || !identifier(actor.companyId))
    return fail(409, 'WORK_CONTEXT_CHANGED');
  const matches = config.holdings.filter(
    item => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
  );
  if (matches.length !== 1) return fail(409, 'WORK_CONTEXT_CHANGED');
  const holding = matches[0];
  if (holding.companyId !== undefined && holding.companyId !== actor.companyId)
    return fail(409, 'WORK_CONTEXT_CHANGED');
  return holding;
}

function post(
  url: URL,
  headersValue: Record<string, string>,
  body: string,
  timeoutMs: number,
): Promise<{ status: number; value: unknown; contentType: string | undefined }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false, maxSockets: 1, maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = httpRequest(url, {
      method: 'POST',
      agent,
      headers: {
        ...headersValue,
        'content-type': 'application/json',
        'content-length': Buffer.byteLength(body),
      },
      signal: AbortSignal.timeout(timeoutMs),
      maxHeaderSize: 8192,
    }, incoming => {
      const chunks: Buffer[] = [];
      incoming.on('data', chunk => chunks.push(Buffer.from(chunk)));
      incoming.on('end', () => {
        agent.destroy();
        const raw = Buffer.concat(chunks).toString('utf8');
        let value: unknown = null;
        try { value = raw ? JSON.parse(raw) : null; } catch { value = null; }
        const header = incoming.headers['content-type'];
        resolve({
          status: incoming.statusCode ?? 502,
          value,
          contentType: Array.isArray(header) ? header[0] : header,
        });
      });
      incoming.on('error', error => { agent.destroy(); reject(error); });
    });
    outgoing.on('error', error => { agent.destroy(); reject(error); });
    outgoing.end(body);
  });
}

/** The sole field-command routing decision, owned end to end by Rust. */
export async function fieldCollaborationCommand(actor: WorkContext, input: unknown): Promise<boolean> {
  if (!input || typeof input !== 'object' || Array.isArray(input)) return false;
  const raw = process.env[FIELD_COMMAND_CONFIG_ENV];
  if (raw === undefined) return unavailable();
  const config = settings(raw);
  const holding = target(config, actor as FieldActor);

  const v = input as Row;
  const entity = v.entity === 'relation' ? 'relation' : 'card';
  const fields: Row = { entity, companyId: actor.companyId };
  if (typeof v.id === 'string') {
    if (!identifier(v.id)) return fail(503, 'NATIVE_COLLABORATION_LANE15_INPUT_INVALID');
    fields.id = v.id;
  } else {
    if (v.sourceType !== undefined && !text(v.sourceType, 64))
      return fail(503, 'NATIVE_COLLABORATION_LANE15_INPUT_INVALID');
    if (v.targetType !== undefined && !text(v.targetType, 64))
      return fail(503, 'NATIVE_COLLABORATION_LANE15_INPUT_INVALID');
    if (v.sourceType !== undefined) fields.sourceType = v.sourceType;
    if (v.targetType !== undefined) fields.targetType = v.targetType;
  }
  const body = JSON.stringify(fields);
  if (new TextEncoder().encode(body).length > 16 * 1024)
    return fail(503, 'NATIVE_COLLABORATION_LANE15_INPUT_INVALID');

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/collaboration/native-lane-15/field-command`,
    config.privateBaseUrl,
  );
  let response: { status: number; value: unknown; contentType: string | undefined };
  try {
    response = await post(url, {
      host: holding.host,
      origin: holding.origin,
      accept: 'application/json',
      'cache-control': 'no-cache',
      'x-erpx-context-company': actor.companyId,
    }, body, config.timeoutMs);
  } catch { return unavailable(); }
  if (response.status !== 200) {
    const error = record(response.value) && record(response.value.error)
      ? response.value.error : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : 'NATIVE_COLLABORATION_LANE15_READ_FAILED',
        text(error?.message, 300) ? String(error!.message) : 'The native field-command read was rejected.',
      );
    return unavailable();
  }
  if (response.contentType?.split(';')[0]?.trim() !== 'application/json') return unavailable();
  const payload = response.value;
  if (!record(payload) || !record(payload.data) ||
    Object.keys(payload.data).length !== 1 ||
    typeof payload.data.fieldOwned !== 'boolean')
    return fail(503, 'NATIVE_COLLABORATION_LANE15_RESPONSE_INVALID');
  return payload.data.fieldOwned;
}
