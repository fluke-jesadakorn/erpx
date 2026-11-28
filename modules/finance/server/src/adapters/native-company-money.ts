import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

/**
 * Fail-closed config-bridge transport for the native Company Money reads.
 * No SQL is authored here; the Rust owner owns every statement. A missing or
 * invalid transport throws an explicit error, never a TS SQL fallback.
 */
export const COMPANY_MONEY_CONFIG_KEY = 'ERPX_NATIVE_FINANCE_COMPANY_MONEY_CONFIG';
const BODY_LIMIT = 2 * 1024 * 1024;
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export type CompanyMoneySettings = Readonly<{ privateBaseUrl: string; timeoutMs: number }>;

export const fail = (status = 503): never => {
  throw new ApiError(
    status,
    'NATIVE_FINANCE_COMPANY_MONEY_UNAVAILABLE',
    'The native Finance Company Money read is unavailable.',
  );
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

export function companyMoneyConfig(): CompanyMoneySettings {
  const raw = process.env[COMPANY_MONEY_CONFIG_KEY];
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (!record(value) || typeof value.privateBaseUrl !== 'string') return fail();
  const timeoutMs = value.timeoutMs;
  if (
    typeof timeoutMs !== 'number' ||
    !Number.isSafeInteger(timeoutMs) ||
    timeoutMs < 250 ||
    timeoutMs > 30000
  )
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
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin
  )
    return fail();
  return { privateBaseUrl: value.privateBaseUrl, timeoutMs };
}

const validIdentifier = (value: string) =>
  value.length > 0 && value.length <= 200 && !/[\u0000-\u001f/?#\\]/.test(value);
const validCurrency = (value: string) => /^[A-Z]{3}$/.test(value);

async function readBody(body: ReadableStream<Uint8Array>): Promise<string> {
  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > BODY_LIMIT) {
        await reader.cancel();
        return fail(502);
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
    offset += chunk.byteLength;
  }
  return new TextDecoder('utf-8', { fatal: true }).decode(data);
}

/** GET a bounded JSON reply from a native Company Money read endpoint. */
export async function companyMoneyGet(
  settings: CompanyMoneySettings,
  companyId: string,
  path: string,
): Promise<unknown> {
  if (!validIdentifier(companyId)) return fail(422);
  const url = new URL(
    `/api/v1/companies/${encodeURIComponent(companyId)}/finance/company-money/${path}`,
    settings.privateBaseUrl,
  );
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET',
      headers: {
        accept: 'application/json',
        'x-erpx-context-company': companyId,
      },
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(settings.timeoutMs),
    });
  } catch {
    return fail(503);
  }
  if (response.redirected || response.status !== 200 || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body.cancel().catch(() => undefined);
    return fail(502);
  }
  try {
    return JSON.parse(await readBody(response.body));
  } catch {
    return fail(502);
  }
}

export { record, validIdentifier, validCurrency };
