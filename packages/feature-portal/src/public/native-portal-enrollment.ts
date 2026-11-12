import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/**
 * Bounded public Customer Portal enrollment client for the native owner.
 * One exact Account origin, one loopback private transport and strict response
 * validation. An unset configuration is a hard failure: the retired TypeScript
 * writer has no fallback.
 */
export type NativePortalEnrollmentConfiguration = Readonly<{
  privateBaseUrl: string;
  accountOrigin: string;
  timeoutMs: number;
}>;

export type NativePortalEnrollmentReservation = Readonly<{
  holdingSlug: string;
  bindingId: string;
  invitationId: string;
  windowStart: number;
}>;

export type NativePortalEnrollmentInvite = Readonly<{
  email: string;
  attempts: number;
  reservation: NativePortalEnrollmentReservation;
}>;

export type NativePortalEnrollmentAcceptance = Readonly<{
  bindingId: string;
  holdingSlug: string;
  accepted: boolean;
}>;

export class NativePortalEnrollmentError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Customer Portal enrollment request could not be completed.');
    this.name = 'NativePortalEnrollmentError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativePortalEnrollmentError(code, status, message);
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const identifier = (value: unknown): value is string =>
  typeof value === 'string' &&
  value.length > 0 &&
  value.length <= 200 &&
  !/[\u0000-\u001f\u007f/\\]/.test(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const emailAddress = (value: unknown): value is string =>
  typeof value === 'string' &&
  value.length > 0 &&
  value.length <= 254 &&
  value === value.toLowerCase() &&
  value.split('@').length === 2 &&
  value.split('@')[1]!.includes('.') &&
  !/[\u0000-\u001f\u007f\s]/.test(value);

function reservation(value: unknown): NativePortalEnrollmentReservation {
  if (
    !exact(value, ['holdingSlug', 'bindingId', 'invitationId', 'windowStart']) ||
    !identifier(value.holdingSlug) ||
    !identifier(value.bindingId) ||
    !identifier(value.invitationId) ||
    !integer(value.windowStart)
  )
    return failure('NATIVE_PORTAL_ENROLLMENT_RESPONSE_INVALID');
  return value as unknown as NativePortalEnrollmentReservation;
}

function invite(value: unknown): NativePortalEnrollmentInvite {
  if (
    !exact(value, ['email', 'attempts', 'reservation']) ||
    !emailAddress(value.email) ||
    !integer(value.attempts) ||
    value.attempts > 1_000
  )
    return failure('NATIVE_PORTAL_ENROLLMENT_RESPONSE_INVALID');
  reservation(value.reservation);
  return value as unknown as NativePortalEnrollmentInvite;
}

function acceptance(value: unknown): NativePortalEnrollmentAcceptance {
  if (
    !exact(value, ['bindingId', 'holdingSlug', 'accepted']) ||
    !identifier(value.bindingId) ||
    !identifier(value.holdingSlug) ||
    value.accepted !== true
  )
    return failure('NATIVE_PORTAL_ENROLLMENT_RESPONSE_INVALID');
  return value as unknown as NativePortalEnrollmentAcceptance;
}

const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativePortalEnrollmentConfiguration) {
  let base: URL;
  let account: URL;
  try {
    base = new URL(raw.privateBaseUrl);
    account = new URL(raw.accountOrigin);
  } catch {
    return failure('NATIVE_PORTAL_ENROLLMENT_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reservedPorts.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin ||
    account.protocol !== 'https:' ||
    account.username ||
    account.password ||
    account.pathname !== '/' ||
    account.search ||
    account.hash ||
    raw.accountOrigin !== account.origin ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30_000
  )
    return failure('NATIVE_PORTAL_ENROLLMENT_CONFIGURATION_INVALID');
  return { base, account, timeout: raw.timeoutMs };
}

function directHttp(
  url: URL,
  headers: Record<string, string>,
  body: string,
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
      { method: 'POST', agent, headers, signal, maxHeaderSize: 8192 },
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
          reject(error);
        }
      },
    );
    outgoing.once('error', () => {
      agent.destroy();
      reject(new NativePortalEnrollmentError('NATIVE_PORTAL_ENROLLMENT_UNAVAILABLE'));
    });
    outgoing.end(body);
  });
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PORTAL_ENROLLMENT_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (response.status !== 200) {
    let code: string | undefined;
    let message: string | undefined;
    if (contentType === 'application/json') {
      try {
        const value: unknown = await response.json();
        if (record(value) && record(value.error)) {
          if (typeof value.error.code === 'string' && value.error.code.length <= 200)
            code = value.error.code;
          if (typeof value.error.message === 'string' && value.error.message.length <= 2000)
            message = value.error.message;
        }
      } catch {
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(code ?? 'NATIVE_PORTAL_ENROLLMENT_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PORTAL_ENROLLMENT_RESPONSE_INVALID');
  }
  const text = await response.text();
  if (text.length > 64 * 1024) return failure('NATIVE_PORTAL_ENROLLMENT_RESPONSE_LIMIT');
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return failure('NATIVE_PORTAL_ENROLLMENT_RESPONSE_INVALID');
  }
}

export function nativePortalEnrollmentConfiguration(
  raw: string,
): NativePortalEnrollmentConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativePortalEnrollmentError('NATIVE_PORTAL_ENROLLMENT_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    typeof value.privateBaseUrl !== 'string' ||
    typeof value.accountOrigin !== 'string' ||
    typeof value.timeoutMs !== 'number'
  )
    return failure('NATIVE_PORTAL_ENROLLMENT_CONFIGURATION_INVALID');
  return value as unknown as NativePortalEnrollmentConfiguration;
}

export function nativePortalEnrollmentAvailable(): boolean {
  return process.env.ERPX_NATIVE_PORTAL_ENROLLMENT_CONFIG !== undefined;
}

function client() {
  const raw = process.env.ERPX_NATIVE_PORTAL_ENROLLMENT_CONFIG;
  if (raw === undefined)
    return failure('NATIVE_PORTAL_ENROLLMENT_CONFIGURATION_REQUIRED');
  return configuration(nativePortalEnrollmentConfiguration(raw));
}

export function createNativePortalEnrollmentClient(
  raw: NativePortalEnrollmentConfiguration,
  transport: typeof directHttp = directHttp,
) {
  const config = configuration(raw);
  const run = async (
    path: string,
    body: unknown,
    headers: Record<string, string>,
    validate: (value: unknown) => unknown,
  ) => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeout);
    try {
      const response = await transport(
        new URL(path, config.base),
        {
          host: config.account.host,
          accept: 'application/json',
          'content-type': 'application/json',
          'cache-control': 'no-cache',
          ...headers,
        },
        JSON.stringify(body),
        controller.signal,
      );
      const value = await boundedJson(response);
      if (!record(value) || !('data' in value) || Object.keys(value).length !== 1)
        return failure('NATIVE_PORTAL_ENROLLMENT_RESPONSE_INVALID');
      return validate(value.data);
    } catch (error) {
      if (error instanceof NativePortalEnrollmentError) throw error;
      return failure('NATIVE_PORTAL_ENROLLMENT_UNAVAILABLE');
    } finally {
      clearTimeout(timer);
    }
  };
  return {
    reserve: (token: string) =>
      run('/api/customer-portal/enroll/reserve', { token }, {}, (value) =>
        invite(value),
      ) as Promise<NativePortalEnrollmentInvite>,
    release: (value: NativePortalEnrollmentReservation) =>
      run('/api/customer-portal/enroll/release', { reservation: value }, {}, (data) => {
        if (!exact(data, ['released']) || data.released !== true)
          return failure('NATIVE_PORTAL_ENROLLMENT_RESPONSE_INVALID');
        return undefined;
      }) as Promise<void>,
    accept: (token: string, cookie: string) => {
      if (cookie.length === 0 || cookie.length > 8192)
        return Promise.reject(
          new NativePortalEnrollmentError('NATIVE_PORTAL_ENROLLMENT_REQUEST_INVALID'),
        );
      return run(
        '/api/customer-portal/enroll/accept',
        { token },
        { cookie },
        (value) => acceptance(value),
      ) as Promise<NativePortalEnrollmentAcceptance>;
    },
  };
}

/** Route-level entry used by the enrollment POST handler. */
export function nativePortalEnrollment() {
  const raw = process.env.ERPX_NATIVE_PORTAL_ENROLLMENT_CONFIG;
  if (raw === undefined) return undefined;
  return createNativePortalEnrollmentClient(nativePortalEnrollmentConfiguration(raw));
}

export function nativePortalEnrollmentFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativePortalEnrollmentError)) return undefined;
  const status =
    error.status ??
    (error.code === 'NATIVE_PORTAL_ENROLLMENT_CONFIGURATION_REQUIRED' ? 503 : 502);
  return {
    status: status === 401 ? 401 : status,
    code: error.code,
    message: error.message,
  };
}
