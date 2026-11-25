import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';

/**
 * Strict private transport for the identity_access Control read-plan port.
 *
 * The axum owner plans one of five provider-context statements; this bridge
 * never emits SQL, accepts no SQL from a caller and has no fallback. The
 * caller executes the returned statement on its own already-open Control unit,
 * so its lease and snapshot semantics are preserved. Missing configuration,
 * transport failure, wrong arity or an unexpected statement hash fails closed.
 */
export const PC_REVOKED_SESSION = 'pc_revoked_session';
export const PC_PLATFORM_ASSIGNMENTS = 'pc_platform_assignments';
export const PC_PROVIDER_CONTEXT = 'pc_provider_context';
export const PC_WORKSPACE_PREFERENCE = 'pc_workspace_preference';
export const PC_CONTEXT_RECEIPT = 'pc_context_receipt';
export const PC_CONNECTION_ROUTE = 'pc_connection_route';
export const PC_MFA_ENABLED = 'pc_mfa_enabled';
export const PC_INVITATION_WAITING = 'pc_invitation_waiting';
export const PC_DELIVERABLE_INVITATION = 'pc_deliverable_invitation';
export const PC_FRESH_MFA = 'pc_fresh_mfa';
export const PC_PREF_BINDING_PROBE = 'pc_pref_binding_probe';
export const PC_PREF_SESSION_SHARE = 'pc_pref_session_share';
export const PC_PREF_SESSION_IMAGE = 'pc_pref_session_image';
export const PC_PREF_BINDING_SHARE = 'pc_pref_binding_share';
export const PC_PREF_PARENT_BINDING_SHARE = 'pc_pref_parent_binding_share';
export const PC_WORKFLOW_ACTIVE_CONTEXT = 'pc_workflow_active_context';
export const PC_QUEUED_ASSIGNMENT_VERSION = 'pc_queued_assignment_version';

export type ControlReadPlan = Readonly<{ sql: string; values: readonly (string | number)[] }>;
type PlanReader = (kind: string, values: readonly (string | number)[]) => Promise<ControlReadPlan>;
type Holding = Readonly<{ origin: string; host: string; holdingSlug: string }>;
type Config = Readonly<{ base: string; timeoutMs: number; holding: Holding }>;

const ENV = 'ERPX_NATIVE_IDENTITY_CONTROL_READ_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const ARITY: Readonly<Record<string, number>> = {
  [PC_REVOKED_SESSION]: 3,
  [PC_PLATFORM_ASSIGNMENTS]: 1,
  [PC_PROVIDER_CONTEXT]: 1,
  [PC_WORKSPACE_PREFERENCE]: 1,
  [PC_CONTEXT_RECEIPT]: 1,
  [PC_CONNECTION_ROUTE]: 1,
  [PC_MFA_ENABLED]: 1,
  [PC_INVITATION_WAITING]: 1,
  [PC_DELIVERABLE_INVITATION]: 1,
  [PC_FRESH_MFA]: 4,
  [PC_PREF_BINDING_PROBE]: 1,
  [PC_PREF_SESSION_SHARE]: 1,
  [PC_PREF_SESSION_IMAGE]: 1,
  [PC_PREF_BINDING_SHARE]: 1,
  [PC_PREF_PARENT_BINDING_SHARE]: 1,
  [PC_WORKFLOW_ACTIVE_CONTEXT]: 2,
  [PC_QUEUED_ASSIGNMENT_VERSION]: 4,
};
/** Byte-oracle SHA-256 of each owner statement: a plan with unexpected bytes is rejected. */
const PLAN_SHAS: Readonly<Record<string, string>> = {
  [PC_REVOKED_SESSION]: 'c8272d0c84cece485749ed6740f81e378d3bca686ff75f3cdc63f71d55e2c628',
  [PC_PLATFORM_ASSIGNMENTS]: '0ac70d35203729f01794acb9d25fb03fa72c36056cfdfce0d965b09301ee38d9',
  [PC_PROVIDER_CONTEXT]: '187ae3a29228e8f2216facd5ee409aca9f7f1035d5ee554c17e412929ea0ff1d',
  [PC_WORKSPACE_PREFERENCE]: 'f14064c0ee3a3034aa871d0594e0e63ca6f889601efef5b37b65ac9009ac5469',
  [PC_CONTEXT_RECEIPT]: 'def58961eb1edadfd240c2b97b615f7421df74e86d1da5bc2bc5f49742298949',
  [PC_CONNECTION_ROUTE]: 'e5b27b09f6631e02ba985c0f48972ac6e38a345bc71f6796d3dc19a93be61001',
  [PC_MFA_ENABLED]: 'ed5645df6420b56f0f1338173e9ae53e3fcc203a3d7fbe165619f396255c5dea',
  [PC_INVITATION_WAITING]: 'c5c89be91182d5cf9ee647729eb1cd2a03c9f4f72bceb7e6ce2a9b5ffadfd0e0',
  [PC_DELIVERABLE_INVITATION]: '5ee384b180a89fa15fe0f47f624cac2d15d84bdc76802e8b48d0df897ece6686',
  [PC_FRESH_MFA]: '65e69ea7d903ef4a7f3d8e622254c9b874cfb825cfd8ca1c29e0ba7577125132',
  [PC_PREF_BINDING_PROBE]: '5893b16f5a1267558051991fecbbd67645fbbee1b6c3e779fcb2ba4141130504',
  [PC_PREF_SESSION_SHARE]: 'be9e3354fe8e8adb7357c1d18e264b4c8d8983ca45951716731a6c42be770be0',
  [PC_PREF_SESSION_IMAGE]: '8ee55872698acd8f8db05e131432e221353aa71e8665b94c8205e012a52bf8af',
  [PC_PREF_BINDING_SHARE]: 'd9ee53a4666438071f1e233fbcb7754912d6f08251b3f69652b2792d9e8a3789',
  [PC_PREF_PARENT_BINDING_SHARE]: '242a5de65756ffa03457eca0a59a03466d074e34c95877f57022dce9b717cce9',
  [PC_WORKFLOW_ACTIVE_CONTEXT]: '98fb96e432fdd1d620164254660524abf2a902e5327498963620cea9d88b94b7',
  [PC_QUEUED_ASSIGNMENT_VERSION]: '3a336e6778cfae07ea33cc8d5d45430ab26fceb78271cd87a68e5fb885f78fc9',
};

const unavailable = (code = 'IDENTITY_CONTROL_READ_UNAVAILABLE', status = 503): never => {
  throw new ApiError(status, code, 'The native identity Control read service is unavailable.');
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 512): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const slug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]{1,128}$/.test(value);
const sha256 = (value: string) => createHash('sha256').update(value).digest('hex');

export function parseIdentityControlReadConfig(raw: string | undefined): Config {
  if (raw === undefined || raw === '')
    return unavailable('IDENTITY_CONTROL_READ_CONFIG_REQUIRED');
  let value: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!object(parsed)) return unavailable('IDENTITY_CONTROL_READ_CONFIG_INVALID');
    value = parsed;
  } catch {
    return unavailable('IDENTITY_CONTROL_READ_CONFIG_INVALID');
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    return unavailable('IDENTITY_CONTROL_READ_CONFIG_INVALID');
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
    base.origin !== String(value.privateBaseUrl) ||
    origin.protocol !== 'https:' ||
    origin.origin !== String(value.origin) ||
    origin.username ||
    origin.password ||
    !Number.isSafeInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable('IDENTITY_CONTROL_READ_CONFIG_INVALID');
  const item = value.holdings[0];
  if (!object(item) || !slug(item.holdingSlug)) return unavailable('IDENTITY_CONTROL_READ_CONFIG_INVALID');
  return {
    base: base.origin,
    timeoutMs: value.timeoutMs as number,
    holding: { origin: origin.origin, host: origin.host, holdingSlug: item.holdingSlug },
  };
}

/** Test seam for the native plan oracle. Production always calls the router. */
let testReader: PlanReader | undefined;
export function setIdentityControlReadForTest(reader?: PlanReader) {
  testReader = reader;
}

function check(kind: string, values: readonly unknown[]): readonly (string | number)[] {
  const arity = ARITY[kind];
  if (arity === undefined || values.length !== arity)
    return unavailable('IDENTITY_CONTROL_READ_INVALID', 422);
  if (kind === PC_FRESH_MFA) {
    const [userId, sessionId, cutoff, isoNow] = values;
    if (
      !text(userId) || !userId ||
      !text(sessionId) || !sessionId ||
      !Number.isSafeInteger(cutoff) || (cutoff as number) <= 0 ||
      !text(isoNow) || !isoNow
    )
      return unavailable('IDENTITY_CONTROL_READ_INVALID', 422);
    return [userId, sessionId, cutoff as number, isoNow];
  }
  if (!values.every((value) => text(value) && value.length > 0))
    return unavailable('IDENTITY_CONTROL_READ_INVALID', 422);
  return values as readonly string[];
}

function verified(plan: unknown, kind: string): ControlReadPlan {
  if (!object(plan) || typeof plan.sql !== 'string' || !Array.isArray(plan.values))
    return unavailable('IDENTITY_CONTROL_READ_RESPONSE_INVALID');
  if (new TextEncoder().encode(plan.sql).length > 1_048_576)
    return unavailable('IDENTITY_CONTROL_READ_RESPONSE_INVALID');
  if (!/^SELECT\s/.test(plan.sql) || sha256(plan.sql) !== PLAN_SHAS[kind])
    return unavailable('IDENTITY_CONTROL_READ_RESPONSE_INVALID');
  const expected = ARITY[kind]!;
  if (plan.values.length !== expected)
    return unavailable('IDENTITY_CONTROL_READ_RESPONSE_INVALID');
  const typed = plan.values.every((value) =>
    typeof value === 'string' || (kind === PC_FRESH_MFA && typeof value === 'number'),
  );
  if (!typed) return unavailable('IDENTITY_CONTROL_READ_RESPONSE_INVALID');
  if (kind !== PC_FRESH_MFA && !plan.values.every((value) => typeof value === 'string'))
    return unavailable('IDENTITY_CONTROL_READ_RESPONSE_INVALID');
  return { sql: plan.sql, values: plan.values as readonly (string | number)[] };
}

/** Plan one registered Control read; execute the reply on the caller's unit. */
export async function planIdentityControlRead(
  kind: string,
  values: readonly unknown[] = [],
  environment: Readonly<Record<string, string | undefined>> = process.env,
): Promise<ControlReadPlan> {
  const bound = check(kind, values);
  if (testReader) return verified(await testReader(kind, bound), kind);
  const config = parseIdentityControlReadConfig(environment[ENV]);
  const body = JSON.stringify({ kind, values: bound });
  if (new TextEncoder().encode(body).length > 8 * 1024)
    return unavailable('IDENTITY_CONTROL_READ_INVALID', 422);
  let response: Response;
  try {
    response = await fetch(
      `${config.base}/api/v1/holdings/${encodeURIComponent(config.holding.holdingSlug)}/identity-access/control-read-plan`,
      {
        method: 'POST',
        headers: {
          host: config.holding.host,
          origin: config.holding.origin,
          accept: 'application/json',
          'content-type': 'application/json',
          'cache-control': 'no-cache',
        },
        body,
        cache: 'no-store',
        redirect: 'error',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    return unavailable('IDENTITY_CONTROL_READ_FAILED');
  }
  const payload: unknown = await response.json().catch(() => null);
  return verified(object(payload) ? payload.data : undefined, kind);
}

/** True when the native Control read transport is configured. */
export function identityControlReadConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
