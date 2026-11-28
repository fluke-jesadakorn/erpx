import 'server-only';
import {
  directHttp,
  type NativeCommerceOrdersTransport,
} from '@erpx/commerce-ui/public/native-commerce-orders';

/** Exact six-column payload of the native employee-request order read. */
export type NativeCommerceEmployeeOrder = Readonly<{
  kind: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  state: string;
  version: number;
}>;

type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

function config(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
  if (raw === undefined)
    throw new NativeCommerceEmployeeOrderError(
      'COMMERCE_EMPLOYEE_ORDER_READ_UNAVAILABLE',
      503,
      'The native Commerce employee-request order service is not configured.',
    );
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
    throw new NativeCommerceEmployeeOrderError(
      'COMMERCE_EMPLOYEE_ORDER_READ_CONFIG_INVALID',
      503,
      'The native Commerce employee-request order transport is misconfigured.',
    );
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new NativeCommerceEmployeeOrderError(
      'COMMERCE_EMPLOYEE_ORDER_READ_CONFIG_INVALID',
      503,
      'The native Commerce employee-request order transport is misconfigured.',
    );
  }
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    origin.protocol !== 'https:' ||
    typeof value.host !== 'string' ||
    value.host !== origin.host ||
    typeof value.holdingSlug !== 'string' ||
    !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000
  )
    throw new NativeCommerceEmployeeOrderError(
      'COMMERCE_EMPLOYEE_ORDER_READ_CONFIG_INVALID',
      503,
      'The native Commerce employee-request order transport is misconfigured.',
    );
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

export class NativeCommerceEmployeeOrderError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'NativeCommerceEmployeeOrderError';
    this.code = code;
    this.status = status;
  }
}

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const KEYS = [
  'kind', 'company_id', 'branch_id', 'department_id', 'state', 'version',
] as const;
const text = (value: unknown, max = 200): value is string =>
  typeof value === 'string' &&
  value.length > 0 &&
  new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && /^[A-Za-z0-9_.:@-]+$/.test(value);

function row(value: unknown): value is NativeCommerceEmployeeOrder {
  if (
    !record(value) ||
    Object.keys(value).length !== KEYS.length ||
    !KEYS.every((key) => Object.hasOwn(value, key))
  )
    return false;
  return (
    text(value.kind, 200) &&
    identifier(value.company_id) &&
    (value.branch_id === null || identifier(value.branch_id)) &&
    (value.department_id === null || identifier(value.department_id)) &&
    text(value.state, 200) &&
    typeof value.version === 'number' &&
    Number.isSafeInteger(value.version) &&
    value.version >= 0
  );
}

async function boundedBytes(response: Response, limit: number): Promise<Uint8Array> {
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > limit) {
        await reader.cancel();
        throw new NativeCommerceEmployeeOrderError(
          'COMMERCE_EMPLOYEE_ORDER_READ_RESPONSE_LIMIT',
          503,
          'The native Commerce employee-request order reply was too large.',
        );
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.length;
  }
  return data;
}

/**
 * Thin, fail-closed transport for the native owner route. It carries no SQL,
 * no fallback and no payload shaping; a malformed or absent native response
 * fails rather than reaching any retained TypeScript reader.
 */
export function nativeCommerceEmployeeOrderReader(
  transport: NativeCommerceOrdersTransport = directHttp,
) {
  const cfg = config();
  const base = new URL(cfg.base);
  return {
    async read(
      companyId: string,
      orderId: string,
      signal?: AbortSignal,
    ): Promise<NativeCommerceEmployeeOrder | null> {
      if (!identifier(companyId) || !identifier(orderId))
        throw new NativeCommerceEmployeeOrderError(
          'COMMERCE_EMPLOYEE_ORDER_READ_AUTHORITY_INVALID',
          422,
          'Review the Commerce employee-request order scope.',
        );
      const controller = new AbortController();
      const cancel = () => controller.abort();
      signal?.addEventListener('abort', cancel, { once: true });
      if (signal?.aborted) controller.abort();
      const timer = setTimeout(cancel, cfg.timeoutMs);
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/commerce/employee-request-order`,
        base,
      );
      const response = await transport(
        'GET',
        url,
        new Headers({
          host: cfg.host,
          origin: cfg.origin,
          accept: 'application/json',
          'cache-control': 'no-cache',
          'x-erpx-context-company': companyId,
          'x-erpx-context-record': orderId,
        }),
        undefined,
        controller.signal,
      ).finally(() => {
        clearTimeout(timer);
        signal?.removeEventListener('abort', cancel);
      });
      if (response.redirected || response.status !== 200) {
        void response.body?.cancel().catch(() => undefined);
        const failure =
          response.status === 422 || response.status === 404
            ? 'COMMERCE_EMPLOYEE_ORDER_READ_FAILED'
            : 'COMMERCE_EMPLOYEE_ORDER_READ_UNAVAILABLE';
        throw new NativeCommerceEmployeeOrderError(failure, response.status, 'The native Commerce employee-request order read failed.');
      }
      if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
        void response.body?.cancel().catch(() => undefined);
        throw new NativeCommerceEmployeeOrderError(
          'COMMERCE_EMPLOYEE_ORDER_READ_RESPONSE_INVALID',
          503,
          'The native Commerce employee-request order reply was invalid.',
        );
      }
      const data = await boundedBytes(response, 64 * 1024);
      let value: unknown;
      try {
        value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
      } catch {
        throw new NativeCommerceEmployeeOrderError(
          'COMMERCE_EMPLOYEE_ORDER_READ_RESPONSE_INVALID',
          503,
          'The native Commerce employee-request order reply was invalid.',
        );
      }
      if (!record(value) || Object.keys(value).length !== 1 || !Object.hasOwn(value, 'data'))
        throw new NativeCommerceEmployeeOrderError(
          'COMMERCE_EMPLOYEE_ORDER_READ_RESPONSE_INVALID',
          503,
          'The native Commerce employee-request order reply was invalid.',
        );
      const payload = value.data;
      if (payload === null) return null;
      if (!row(payload))
        throw new NativeCommerceEmployeeOrderError(
          'COMMERCE_EMPLOYEE_ORDER_READ_RESPONSE_INVALID',
          503,
          'The native Commerce employee-request order reply was invalid.',
        );
      return payload;
    },
  };
}
