import 'server-only';

/** Strict transport for the native People workforce directory command. */
export type NativeWorkforceConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
}>;

export type NativeWorkforcePageContext = Readonly<{
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  assignmentScope: string;
  authority?: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam?: { permissions: readonly string[] };
}>;

export class NativeWorkforceError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native People directory command could not be completed.');
    this.name = 'NativeWorkforceError';
    this.code = code;
    this.status = status;
  }
}

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max &&
  value.length > 0;

export function nativeWorkforceConfigured(): boolean {
  return process.env.ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG !== undefined;
}

export function nativeWorkforceConfiguration(raw: string): NativeWorkforceConfiguration {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { throw new NativeWorkforceError('NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID'); }
  if (!record(value) || !text(value.privateBaseUrl, 2000) || !Number.isInteger(value.timeoutMs)
    || Number(value.timeoutMs) < 250 || Number(value.timeoutMs) > 30000
    || !Array.isArray(value.holdings) || value.holdings.length === 0)
    throw new NativeWorkforceError('NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID');
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { throw new NativeWorkforceError('NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID'); }
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port
    || base.username || base.password || value.privateBaseUrl !== base.origin)
    throw new NativeWorkforceError('NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID');
  for (const holding of value.holdings) {
    if (!record(holding) || !text(holding.holdingSlug, 100) || !text(holding.holdingId, 100) || !text(holding.origin, 2000))
      throw new NativeWorkforceError('NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID');
  }
  return value as unknown as NativeWorkforceConfiguration;
}

function forwarded(incoming: Pick<Headers, 'get'>, config: NativeWorkforceConfiguration) {
  const host = incoming.get('host');
  const holding = config.holdings.find(item => {
    try { return new URL(item.origin).host === host; } catch { return false; }
  });
  if (!holding) throw new NativeWorkforceError('NOT_FOUND', 404);
  const headers = new Headers({
    host: new URL(holding.origin).host,
    accept: 'application/json',
    'content-type': 'application/json',
  });
  for (const name of ['cookie', 'origin', 'idempotency-key', 'x-erpx-context-company',
    'x-erpx-context-assignment', 'x-erpx-context-revision', 'x-erpx-context-assignment-version',
    'x-erpx-workspace-branch', 'x-erpx-workspace-department']) {
    const value = incoming.get(name);
    if (value !== null) headers.set(name, value);
  }
  return headers;
}

export function createNativeWorkforceClient(config: NativeWorkforceConfiguration) {
  return async function command(
    incoming: Pick<Headers, 'get'>,
    actor: NativeWorkforcePageContext,
    holdingSlug: string,
    personId: string | null,
    body: string,
    key: string,
  ): Promise<unknown> {
    if (actor.holdingSlug !== holdingSlug || !actor.authority || !actor.iam)
      throw new NativeWorkforceError('NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED');
    const host = incoming.get('host');
    const target = config.holdings.find(item => {
      try { return new URL(item.origin).host === host; } catch { return false; }
    });
    if (!target || target.holdingSlug !== holdingSlug || target.holdingId !== actor.holdingId)
      throw new NativeWorkforceError('NOT_FOUND', 404);
    const url = `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/people/directory${personId ? `/${encodeURIComponent(personId)}` : ''}`;
    let response: Response;
    try {
      response = await fetch(url, {
        method: 'POST',
        headers: forwarded(incoming, config),
        body,
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeoutMs),
      });
    } catch { throw new NativeWorkforceError('NATIVE_PEOPLE_PAYROLL_UNAVAILABLE', 503); }
    const payload: unknown = await response.json().catch(() => null);
    if (!response.ok) {
      const error = record(payload) && record(payload.error) ? payload.error : null;
      const code = error && text(error.code, 100) ? error.code : 'NATIVE_PEOPLE_PAYROLL_FAILED';
      const message = error && text(error.message, 1000) ? error.message : 'The native People directory command failed.';
      throw new NativeWorkforceError(code, response.status, message);
    }
    if (!record(payload) || !record(payload.data) || !record(payload.context))
      throw new NativeWorkforceError('NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID', 502);
    return payload.data;
  };
}

export function nativeWorkforceFailure(error: unknown): { status: number; code: string; message: string } | null {
  return error instanceof NativeWorkforceError
    ? { status: error.status ?? 502, code: error.code, message: error.message }
    : null;
}
