import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Strict private transport for the private-file denial authority witness.
 *
 * The native axum/sqlx owner owns the closed IAM/Organization/People image and
 * the opaque equality token. This bridge carries no query text, accepts no
 * query text from a caller and fails closed when the native runtime is missing,
 * malformed or unavailable. Target selection is an exact (holdingId,
 * holdingSlug) match; there is no first-holding default.
 */
type WitnessActor = Readonly<{
  companyId: string;
  userId: string;
  assignmentId: string;
  role: string;
  sessionId: string | null;
}>;
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
}>;
type Config = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;
type WitnessReader = (actor: WitnessActor, holding: Holding) => Promise<string>;

const ENV = 'ERPX_NATIVE_PRIVATE_FILE_DENIAL_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (code = 'PRIVATE_FILE_DENIAL_AUTHORITY_UNAVAILABLE', status = 503): never => {
  throw new ApiError(status, code, 'The native private-file denial authority read is unavailable.');
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 512): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const id = (value: unknown, max = 200): value is string =>
  text(value, max) && /^[A-Za-z0-9_.:-]+$/.test(value);
const slug = (value: unknown, max = 128): value is string =>
  id(value, max) && /^[a-z0-9_-]+$/.test(value);
const witnessShape = (value: unknown): value is string =>
  typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);

function configuration(): Config {
  const raw = process.env[ENV];
  if (!raw) return unavailable('PRIVATE_FILE_DENIAL_AUTHORITY_CONFIGURATION_REQUIRED');
  let value: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!object(parsed)) return unavailable('PRIVATE_FILE_DENIAL_AUTHORITY_CONFIGURATION_INVALID');
    value = parsed;
  } catch {
    return unavailable('PRIVATE_FILE_DENIAL_AUTHORITY_CONFIGURATION_INVALID');
  }
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    return unavailable('PRIVATE_FILE_DENIAL_AUTHORITY_CONFIGURATION_INVALID');
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
    !Number.isSafeInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable('PRIVATE_FILE_DENIAL_AUTHORITY_CONFIGURATION_INVALID');
  const holdings: Holding[] = [];
  const seen = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !id(item.holdingId) ||
      !slug(item.holdingSlug)
    )
      return unavailable('PRIVATE_FILE_DENIAL_AUTHORITY_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable('PRIVATE_FILE_DENIAL_AUTHORITY_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      (item.host !== undefined && !text(item.host, 512))
    )
      return unavailable('PRIVATE_FILE_DENIAL_AUTHORITY_CONFIGURATION_INVALID');
    const host = origin.host;
    const key = `${host}\u0000${item.holdingSlug}`;
    if (seen.has(key)) return unavailable('PRIVATE_FILE_DENIAL_AUTHORITY_CONFIGURATION_INVALID');
    seen.add(key);
    holdings.push({
      origin: origin.origin,
      host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

function held(actor: WorkContext): { actor: WitnessActor; holding: Holding } {
  if (
    !id(actor.companyId) ||
    !id(actor.userId) ||
    !id(actor.assignmentId) ||
    typeof actor.role !== 'string' ||
    actor.role.length > 200 ||
    !id(actor.holdingId) ||
    !slug(actor.holdingSlug)
  )
    return unavailable('PRIVATE_FILE_DENIAL_REQUEST_INVALID', 422);
  const sessionId = actor.authority?.sessionId ?? null;
  if (sessionId !== null && !id(sessionId))
    return unavailable('PRIVATE_FILE_DENIAL_REQUEST_INVALID', 422);
  const found = configuration().holdings.filter(
    (item) => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
  );
  if (found.length !== 1) return unavailable('PRIVATE_FILE_DENIAL_AUTHORITY_TARGET_UNAVAILABLE');
  return {
    actor: {
      companyId: actor.companyId,
      userId: actor.userId,
      assignmentId: actor.assignmentId,
      role: actor.role,
      sessionId,
    },
    holding: found[0]!,
  };
}

/** Test seam for the native witness oracle. Production always calls the router. */
let testReader: WitnessReader | undefined;
export function setPrivateFileDenialWitnessForTest(reader?: WitnessReader) {
  testReader = reader;
}

async function nativeWitness(actor: WorkContext): Promise<string> {
  const target = held(actor);
  if (testReader) {
    const witness = await testReader(target.actor, target.holding);
    if (!witnessShape(witness)) return unavailable('PRIVATE_FILE_DENIAL_RESPONSE_INVALID');
    return witness;
  }
  const config = configuration();
  const body = JSON.stringify(target.actor);
  if (new TextEncoder().encode(body).length > 4 * 1024)
    return unavailable('PRIVATE_FILE_DENIAL_REQUEST_INVALID', 422);
  const headers = new Headers({
    host: target.holding.host,
    origin: target.holding.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': target.actor.companyId,
  });
  let response: Response;
  try {
    response = await fetch(
      `${config.base}/api/v1/holdings/${encodeURIComponent(target.holding.holdingSlug)}/identity-access/private-file-denial-witness`,
      {
        method: 'POST',
        headers,
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
    const payload = await response.json().catch((): unknown => null);
    const error = object(payload) && object(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : 'PRIVATE_FILE_DENIAL_FAILED',
        text(error?.message, 300) ? String(error!.message) : 'The private-file denial read was rejected.',
      );
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const witness =
    object(payload) && object(payload.data) ? payload.data.witness : undefined;
  if (!witnessShape(witness)) return unavailable('PRIVATE_FILE_DENIAL_RESPONSE_INVALID');
  return witness;
}

/** Preserves the owner's source lease: the native owner supplies the facts. */
export function privateFileDenialAuthoritySourceLocks(database: SqlDatabase) {
  return [database.prepare('LOCK TABLE iam_permissions IN SHARE MODE')];
}

/** Native successor of the deleted TS image query; same owner-facing contract. */
export async function privateFileDenialAuthorityWitness(
  _database: SqlDatabase,
  actor: WorkContext,
): Promise<string> {
  return nativeWitness(actor);
}
