import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/**
 * Native lane-32 Benefits transport for the released payroll-input read. This
 * is the sole successor of `benefitPayrollInputsSql`: it carries no SQL text,
 * accepts no SQL from a caller and has no TypeScript fallback. A missing,
 * malformed or failing native owner fails closed with ApiError(503, ...).
 */
export const NATIVE_BENEFITS_LANE32_CONFIG = 'ERPX_NATIVE_BENEFITS_LANE32_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export type NativeBenefitsLane32Config = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
  companyId?: string;
}>;

export type PayrollInputArgs = Readonly<{
  engagementId: string;
  currency: string;
  periodEnd: string;
  periodStart: string;
}>;

export type PayrollInputRow = Readonly<{
  id: string;
  plan_name: string;
  employee_deduction_minor: number;
  employer_cost_minor: number;
  source_election_version: number;
}>;

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

/** Strict private configuration. Only loopback, non-reserved ports are trusted. */
export function nativeBenefitsLane32Configuration(
  raw: string | undefined,
): NativeBenefitsLane32Config {
  if (!raw) return unavailable('BENEFIT_PAYROLL_INPUT_READ_UNAVAILABLE', 'The native Benefits payroll-input service is not configured.');
  let value: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!record(parsed)) throw new Error('not an object');
    value = parsed;
  } catch {
    return unavailable('BENEFIT_PAYROLL_INPUT_READ_CONFIG_INVALID', 'The native Benefits payroll-input transport is misconfigured.');
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
    value.timeoutMs > 30_000 ||
    (value.companyId !== undefined && !text(value.companyId, 200))
  )
    return unavailable('BENEFIT_PAYROLL_INPUT_READ_CONFIG_INVALID', 'The native Benefits payroll-input transport is misconfigured.');
  let base: URL;
  let origin: URL;
  try {
    base = new URL(value.privateBaseUrl);
    origin = new URL(value.origin);
  } catch {
    return unavailable('BENEFIT_PAYROLL_INPUT_READ_CONFIG_INVALID', 'The native Benefits payroll-input transport is misconfigured.');
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
    base.origin !== value.privateBaseUrl ||
    origin.protocol !== 'https:' ||
    origin.origin !== value.origin ||
    origin.host !== value.host ||
    origin.username ||
    origin.password
  )
    return unavailable('BENEFIT_PAYROLL_INPUT_READ_CONFIG_INVALID', 'The native Benefits payroll-input transport is misconfigured.');
  const companyId = typeof value.companyId === 'string' ? value.companyId : undefined;
  if (companyId !== undefined && !/^[A-Za-z0-9_.:-]{1,200}$/.test(companyId))
    return unavailable('BENEFIT_PAYROLL_INPUT_READ_CONFIG_INVALID', 'The native Benefits payroll-input transport is misconfigured.');
  return {
    base: base.origin,
    host: origin.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs,
    ...(companyId !== undefined ? { companyId } : {}),
  };
}

/** GET with an explicit Host header; `fetch` forbids overriding Host. */
function httpGet(
  url: URL,
  headers: Record<string, string>,
  timeoutMs: number,
): Promise<{ status: number; value: unknown }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      {
        method: 'GET',
        agent,
        headers: { ...headers },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on('data', chunk => chunks.push(Buffer.from(chunk)));
        incoming.on('end', () => {
          agent.destroy();
          const body = Buffer.concat(chunks).toString('utf8');
          let value: unknown = null;
          try {
            value = body ? JSON.parse(body) : null;
          } catch {
            value = null;
          }
          resolve({ status: incoming.statusCode ?? 502, value });
        });
        incoming.on('error', error => {
          agent.destroy();
          reject(error);
        });
      },
    );
    outgoing.on('error', error => {
      agent.destroy();
      reject(error);
    });
    outgoing.end();
  });
}

function valid(value: unknown): value is PayrollInputRow {
  return (
    record(value) &&
    Object.keys(value).length === 5 &&
    typeof value.id === 'string' &&
    typeof value.plan_name === 'string' &&
    Number.isSafeInteger(value.employee_deduction_minor) &&
    Number.isSafeInteger(value.employer_cost_minor) &&
    Number.isSafeInteger(value.source_election_version)
  );
}

/** Strictly validate the native reply; any deviation fails closed. */
export function readBenefitPayrollInputsReply(value: unknown): PayrollInputRow[] {
  if (
    !record(value) ||
    Object.keys(value).length !== 1 ||
    !Array.isArray(value.inputs) ||
    !value.inputs.every(valid)
  )
    return unavailable('BENEFIT_PAYROLL_INPUT_READ_RESPONSE_INVALID', 'The native Benefits payroll-input reply was invalid.');
  return value.inputs as PayrollInputRow[];
}

/** Injectable client used by the contract test and the owner adapters. */
export function createBenefitPayrollInputsClient(
  cfg: NativeBenefitsLane32Config,
  transport: Transport,
) {
  return async function read(args: PayrollInputArgs, companyId?: string): Promise<PayrollInputRow[]> {
    const scope = companyId ?? cfg.companyId;
    if (
      !scope ||
      !/^[A-Za-z0-9_.:-]{1,200}$/.test(scope) ||
      !/^[A-Za-z0-9_.:-]{1,200}$/.test(args.engagementId) ||
      !/^[A-Z]{3}$/.test(args.currency) ||
      !/^\d{4}-\d{2}-\d{2}$/.test(args.periodStart) ||
      !/^\d{4}-\d{2}-\d{2}$/.test(args.periodEnd) ||
      args.periodStart > args.periodEnd
    )
      return unavailable('BENEFIT_PAYROLL_INPUT_READ_INVALID', 'Use one bounded Benefits payroll-input read.');
    let response: { status: number; value: unknown };
    try {
      response = await transport(
        new URL(
          `/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/benefits/payroll-inputs`,
          cfg.base,
        ),
        {
          host: cfg.host,
          origin: cfg.origin,
          accept: 'application/json',
          'cache-control': 'no-cache',
          'x-erpx-context-company': scope,
          'x-erpx-benefits-engagement': args.engagementId,
          'x-erpx-benefits-currency': args.currency,
          'x-erpx-benefits-period-start': args.periodStart,
          'x-erpx-benefits-period-end': args.periodEnd,
        },
        cfg.timeoutMs,
      );
    } catch {
      return unavailable('BENEFIT_PAYROLL_INPUT_READ_UNAVAILABLE', 'The native Benefits payroll-input service is unavailable.');
    }
    if (response.status !== 200)
      return unavailable('BENEFIT_PAYROLL_INPUT_READ_UNAVAILABLE', 'The native Benefits payroll-input service is unavailable.');
    return readBenefitPayrollInputsReply(response.value);
  };
}

/**
 * Native-backed released payroll-input reader. `database` is retained for
 * call-site compatibility only; no SQL is issued on it.
 */
export async function benefitPayrollInputs(
  _database: SqlDatabase,
  args: PayrollInputArgs,
  companyId?: string,
): Promise<{ results: PayrollInputRow[] }> {
  const cfg = nativeBenefitsLane32Configuration(process.env[NATIVE_BENEFITS_LANE32_CONFIG]);
  const rows = await createBenefitPayrollInputsClient(cfg, httpGet)(args, companyId);
  return { results: rows };
}
