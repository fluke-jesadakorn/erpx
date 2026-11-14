import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type {
  CustomerNoteCommandResult,
  CustomerNotesView,
} from '../public/customer-notes';

type Configuration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Incoming = Pick<Headers, 'get'>;

export class NativeCustomerNotesError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native customer notes request could not be completed.');
    this.name = 'NativeCustomerNotesError';
    this.code = code;
    this.status = status;
  }
}

const fail = (code: string, status?: number): never => {
  throw new NativeCustomerNotesError(code, status);
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, maximum = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= maximum;
const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;
const nullable = (value: unknown) => value === null || text(value);
const strings = (value: unknown) =>
  Array.isArray(value) && value.every((item) => text(item));

function configuration(raw: string): Configuration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail('NATIVE_CUSTOMER_NOTES_CONFIGURATION_INVALID');
  }
  if (
    !object(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs, 250) ||
    value.timeoutMs > 30000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return fail('NATIVE_CUSTOMER_NOTES_CONFIGURATION_INVALID');
  const base = new URL(value.privateBaseUrl);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    [3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080].includes(
      Number(base.port),
    ) ||
    (Number(base.port) >= 3200 && Number(base.port) <= 3299) ||
    value.privateBaseUrl !== base.origin
  )
    return fail('NATIVE_CUSTOMER_NOTES_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of value.holdings) {
    if (
      !object(holding) ||
      !text(holding.origin) ||
      !text(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug)
    )
      return fail('NATIVE_CUSTOMER_NOTES_CONFIGURATION_INVALID');
    const origin = new URL(holding.origin);
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      holdings.has(origin.host)
    )
      return fail('NATIVE_CUSTOMER_NOTES_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return {
    privateBaseUrl: value.privateBaseUrl,
    holdings: value.holdings as Configuration['holdings'],
    timeoutMs: value.timeoutMs,
  };
}

function context(actor: WorkContext) {
  if (!actor.authority || !actor.iam)
    return fail('NATIVE_CUSTOMER_NOTES_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: actor.authority.revision,
    assignmentVersion: actor.authority.assignmentVersion,
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function directHttp(
  method: 'GET' | 'POST',
  url: URL,
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
    const outgoing = request(
      url,
      {
        method,
        agent,
        headers: { ...Object.fromEntries(headers) },
        signal,
        maxHeaderSize: 8192,
      },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          const values = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: status >= 200 && status <= 599 ? status : 502,
              headers: values,
            }),
          );
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', (error) => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(body);
  });
}

async function boundedJson(response: Response): Promise<unknown> {
  if (
    response.headers.get('content-type')?.split(';')[0]?.trim() !==
      'application/json' ||
    !response.body
  ) {
    void response.body?.cancel().catch(() => undefined);
    return fail('NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID');
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > 256 * 1024) {
      await reader.cancel();
      return fail('NATIVE_CUSTOMER_NOTES_RESPONSE_LIMIT');
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  } catch {
    return fail('NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID');
  }
}

function page(value: unknown): CustomerNotesView {
  if (!object(value)) return fail('NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID');
  const customer = value.customer;
  if (
    !object(customer) ||
    !text(customer.id) ||
    !text(customer.name) ||
    !text(customer.code) ||
    !integer(customer.version, 1) ||
    typeof value.canCreate !== 'boolean' ||
    !Array.isArray(value.notes) ||
    !Array.isArray(value.history)
  )
    return fail('NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID');
  const order = value.order;
  if (
    order !== undefined &&
    (!object(order) ||
      !text(order.id) ||
      !text(order.party_id) ||
      !text(order.party_name) ||
      !text(order.number) ||
      !integer(order.version, 1))
  )
    return fail('NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID');
  for (const note of value.notes) {
    if (
      !object(note) ||
      !text(note.id) ||
      !text(note.company_id) ||
      !text(note.customer_id) ||
      !nullable(note.order_id) ||
      !text(note.channel) ||
      !integer(note.occurred_at) ||
      !text(note.body) ||
      !text(note.status) ||
      !integer(note.version, 1) ||
      !text(note.created_by) ||
      !integer(note.created_at) ||
      !integer(note.updated_at) ||
      typeof note.canChange !== 'boolean'
    )
      return fail('NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID');
  }
  for (const row of value.history) {
    if (
      !object(row) ||
      !text(row.id) ||
      !text(row.note_id) ||
      !integer(row.version, 1) ||
      !text(row.action) ||
      !text(row.body) ||
      !text(row.reason) ||
      !text(row.actor_id) ||
      !integer(row.created_at)
    )
      return fail('NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID');
  }
  return value as unknown as CustomerNotesView;
}

export function createNativeCustomerNotesClient(
  raw: string,
  send: typeof directHttp = directHttp,
) {
  const config = configuration(raw);
  return async (
    incoming: Incoming,
    actor: WorkContext,
    input: { customerId?: string; orderId?: string },
  ): Promise<CustomerNotesView> => {
    const forwarded = incoming.get('x-forwarded-host')?.split(',')[0]?.trim();
    const host = forwarded && forwarded.length > 0 ? forwarded : incoming.get('host');
    const target = host
      ? config.holdings.find(
          (holding) => new URL(holding.origin).host === host,
        )
      : undefined;
    if (
      !target ||
      target.holdingSlug !== actor.holdingSlug ||
      target.holdingId !== actor.holdingId ||
      (input.customerId === undefined) === (input.orderId === undefined)
    )
      return fail('NATIVE_CUSTOMER_NOTES_AUTHORITY_INVALID');
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/customer-notes`,
      config.privateBaseUrl,
    );
    if (input.customerId) url.searchParams.set('customerId', input.customerId);
    if (input.orderId) url.searchParams.set('orderId', input.orderId);
    const headers = new Headers({
      host: new URL(target.origin).host,
      accept: 'application/json',
      'cache-control': 'no-cache',
      origin: target.origin,
    });
    const cookie = incoming.get('cookie');
    if (cookie) {
      if (new TextEncoder().encode(cookie).length > 8192)
        return fail('NATIVE_CUSTOMER_NOTES_COOKIE_LIMIT');
      headers.set('cookie', cookie);
    }
    const response = await send(
      'GET',
      url,
      headers,
      undefined,
      AbortSignal.timeout(config.timeoutMs),
    ).catch(() => fail('NATIVE_CUSTOMER_NOTES_UNAVAILABLE'));
    const value = await boundedJson(response);
    if (response.status !== 200) {
      if (object(value) && object(value.error) && text(value.error.code))
        return fail(value.error.code, response.status);
      return fail('NATIVE_CUSTOMER_NOTES_UNAVAILABLE', response.status);
    }
    if (!object(value) || !('data' in value))
      return fail('NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID');
    return page(value.data);
  };
}

export function createNativeCustomerNotesCommandClient(
  raw: string,
  send: typeof directHttp = directHttp,
) {
  const config = configuration(raw);
  return async (
    incoming: Incoming,
    actor: WorkContext,
    input: Record<string, unknown>,
    key: string,
  ): Promise<CustomerNoteCommandResult> => {
    const forwarded = incoming.get('x-forwarded-host')?.split(',')[0]?.trim();
    const host = forwarded && forwarded.length > 0 ? forwarded : incoming.get('host');
    const target = host
      ? config.holdings.find(
          (holding) => new URL(holding.origin).host === host,
        )
      : undefined;
    if (!target || target.holdingSlug !== actor.holdingSlug || target.holdingId !== actor.holdingId)
      return fail('NATIVE_CUSTOMER_NOTES_AUTHORITY_INVALID');
    const ctx = context(actor);
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/customer-notes`,
      config.privateBaseUrl,
    );
    const headers = new Headers({
      host: new URL(target.origin).host,
      accept: 'application/json',
      'content-type': 'application/json',
      origin: target.origin,
      'idempotency-key': key,
      'x-erpx-context-company': ctx.companyId,
      'x-erpx-context-assignment': ctx.assignmentId,
      'x-erpx-context-revision': String(ctx.revision),
      'x-erpx-context-assignment-version': String(ctx.assignmentVersion),
    });
    if (ctx.branchSlug) headers.set('x-erpx-branch-slug', ctx.branchSlug);
    if (ctx.departmentSlug) headers.set('x-erpx-department-slug', ctx.departmentSlug);
    const cookie = incoming.get('cookie');
    if (cookie) {
      if (new TextEncoder().encode(cookie).length > 8192)
        return fail('NATIVE_CUSTOMER_NOTES_COOKIE_LIMIT');
      headers.set('cookie', cookie);
    }
    const response = await send(
      'POST',
      url,
      headers,
      JSON.stringify(input),
      AbortSignal.timeout(config.timeoutMs),
    ).catch(() => fail('NATIVE_CUSTOMER_NOTES_UNAVAILABLE'));
    const value = await boundedJson(response);
    if (response.status !== 200) {
      if (object(value) && object(value.error) && text(value.error.code))
        return fail(value.error.code, response.status);
      return fail('NATIVE_CUSTOMER_NOTES_UNAVAILABLE', response.status);
    }
    if (!object(value) || !object(value.data))
      return fail('NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID');
    const result = value.data;
    if (!text(result.id) || !integer(result.version, 1) || !text(result.status))
      return fail('NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID');
    return result as unknown as CustomerNoteCommandResult;
  };
}

export function nativeCustomerNotesConfiguration(raw: string) {
  return configuration(raw);
}
