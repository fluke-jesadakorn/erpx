import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/** Closed beta_policy point-read kinds resolved by the mounted owner
 * `/manage/command-plan` route. The SQL lives only in the Rust registry. */
export const ME_RD_BETA_POLICY_POINT_ALL = 'manage_rd_beta_policy_point_all';
export const ME_RD_BETA_POLICY_POINT_MEMBER = 'manage_rd_beta_policy_point_member_limit';
export const ME_RD_BETA_POLICY_POINT_BRANCH = 'manage_rd_beta_policy_point_branch_limit';

/**
 * Strict owner transport for the mounted native Manage ERPX `command-plan`
 * read route. The axum owner plans the exact statement for a registered read
 * kind; this bridge emits no SQL, accepts no SQL from a caller, and executes
 * the returned statement on the caller's already-open unit. Read kinds carry
 * no receipt/audit/outbox ledger. Missing configuration or any failed
 * configured request fails closed with no TypeScript fallback.
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

export class NativeManageOwnerReadError extends Error {
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native Manage ERPX owner read could not be completed.');
    this.name = 'NativeManageOwnerReadError';
    this.status = status;
  }
}

const fail = (status?: number): never => {
  throw new NativeManageOwnerReadError('NATIVE_MANAGE_ERPX_OWNER_READ_UNAVAILABLE', status);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): NativeConfig {
  // The read route is mounted next to the owner-write route on the same
  // private Holding gateway, so it reuses the exact validated configuration.
  const raw = process.env.ERPX_NATIVE_MANAGE_ERPX_OWNER_WRITE_CONFIG;
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (
    !record(value) ||
    typeof value.privateBaseUrl !== 'string' ||
    typeof value.holdingSlug !== 'string' ||
    typeof value.origin !== 'string' ||
    typeof value.timeoutMs !== 'number'
  )
    return fail();
  if (
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug)
  )
    return fail();
  let base: URL;
  let origin: URL;
  try {
    base = new URL(value.privateBaseUrl);
    origin = new URL(value.origin);
  } catch {
    return fail();
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
    value.privateBaseUrl !== base.origin ||
    origin.protocol !== 'https:' ||
    origin.username ||
    origin.password ||
    value.origin !== origin.origin
  )
    return fail();
  return {
    base: base.origin,
    host: origin.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs,
  };
}

/** Explicit Host header; fetch forbids overriding it. */
function post(
  url: URL,
  headers: Record<string, string>,
  body: string,
  timeoutMs: number,
): Promise<{ status: number; text: string }> {
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
        method: 'POST',
        agent,
        headers: { ...headers, 'content-length': String(new TextEncoder().encode(body).length) },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        incoming.once('close', () => agent.destroy());
        const chunks: Buffer[] = [];
        incoming.on('data', chunk => chunks.push(Buffer.from(chunk as Uint8Array)));
        incoming.on('end', () => {
          resolve({ status: incoming.statusCode ?? 502, text: Buffer.concat(chunks).toString('utf8') });
        });
        incoming.on('error', error => {
          agent.destroy();
          reject(error);
        });
      },
    );
    outgoing.once('error', error => {
      agent.destroy();
      reject(error);
    });
    outgoing.write(body);
    outgoing.end();
  });
}

type Planned = { sql: string; values: readonly unknown[] };

/** Ask the owner to plan one read; only a registered kind and its bounded
 * positional values cross the wire. The local bind-count map and the owner
 * registry must agree, and exactly one statement is accepted. */
async function plan(kind: string, values: readonly unknown[]): Promise<Planned> {
  const cfg = configuration();
  const bindCount = BIND_COUNTS[kind];
  if (!/^[a-z0-9_]{1,120}$/.test(kind) || bindCount === undefined || values.length !== bindCount)
    return fail();
  let response: { status: number; text: string };
  try {
    response = await post(
      new URL(`/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/manage/command-plan`, cfg.base),
      {
        host: cfg.host,
        origin: cfg.origin,
        accept: 'application/json',
        'content-type': 'application/json',
        'cache-control': 'no-cache',
      },
      JSON.stringify({ kind, values }),
      cfg.timeoutMs,
    );
  } catch {
    return fail();
  }
  if (response.status !== 200 || response.text.length > 16 * 1024) return fail(response.status);
  let payload: unknown;
  try {
    payload = JSON.parse(response.text);
  } catch {
    return fail();
  }
  if (!record(payload) || !record(payload.data) || !Array.isArray(payload.data.statements))
    return fail();
  const statements = payload.data.statements;
  if (statements.length !== 1) return fail();
  const statement: unknown = statements[0];
  if (
    !record(statement) ||
    typeof statement.sql !== 'string' ||
    !Array.isArray(statement.values) ||
    statement.values.length !== bindCount
  )
    return fail();
  return { sql: statement.sql, values: statement.values };
}

/** Planned owner read returning a single row (retired `.first()` parity). */
export async function nativeManageOwnerReadFirst<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T | null> {
  const statement = await plan(kind, values);
  return db.prepare(statement.sql).bind(...statement.values).first<T>();
}

/** Closed point-by-key read kinds resolved by the mounted owner plan route.
 * The byte-identical statements live only in the Rust registry. */
export const ME_RD_SLOT_ACTIVE = 'manage_rd_slot_active';
export const ME_RD_SLOT_OWNER = 'manage_rd_slot_owner';
export const ME_RD_INVITATION_BY_TOKEN = 'manage_rd_invitation_by_token';
export const ME_RD_INVITATION_ACCEPTED_USER = 'manage_rd_invitation_accepted_user';
export const ME_RD_COMPANY_PROVISIONING_ATTACHMENT_IMAGE =
  'manage_rd_company_provisioning_attachment_image';

/** Closed kind-to-bind-count map. No unregistered kind reaches the owner. */
const BIND_COUNTS: Readonly<Partial<Record<string, number>>> = {
  [ME_RD_BETA_POLICY_POINT_ALL]: 1,
  [ME_RD_BETA_POLICY_POINT_MEMBER]: 1,
  [ME_RD_BETA_POLICY_POINT_BRANCH]: 1,
  [ME_RD_SLOT_ACTIVE]: 1,
  [ME_RD_SLOT_OWNER]: 1,
  [ME_RD_INVITATION_BY_TOKEN]: 1,
  [ME_RD_INVITATION_ACCEPTED_USER]: 1,
  [ME_RD_COMPANY_PROVISIONING_ATTACHMENT_IMAGE]: 2,
};

const identifier = (value: unknown): value is string =>
  typeof value === 'string' &&
  value.length > 0 &&
  value.length <= 200 &&
  !/[\u0000-\u001f\u007f/\\]/.test(value);

type SlotRow = {
  slug: string;
  company_name: string;
  branch_name: string;
  state: string;
};
type InvitationRow = {
  id: string;
  email: string;
  branch_id: string;
  invited_by: string;
  expires_at: number;
  accepted_user_id: string | null;
  revoked_at: number | null;
  role: 'employee' | 'it_admin';
};

/** Native active-slot probe. Only the bounded holding id crosses the wire. */
export async function nativeBetaSlotActive(db: SqlDatabase, holdingId: string): Promise<boolean> {
  if (!identifier(holdingId)) return fail();
  return (await nativeManageOwnerReadFirst(ME_RD_SLOT_ACTIVE, [holdingId], db)) !== null;
}

/** Native onboarding entrypoint row. The owner owns the projection and key. */
export async function nativeOwnerOnboardingSlot(
  db: SqlDatabase,
  userId: string,
): Promise<SlotRow | null> {
  if (!identifier(userId)) return fail();
  const row = await nativeManageOwnerReadFirst<SlotRow>(ME_RD_SLOT_OWNER, [userId], db);
  if (row === null) return null;
  if (
    !identifier(row.slug) ||
    !identifier(row.company_name) ||
    !identifier(row.branch_name) ||
    !identifier(row.state)
  )
    return fail();
  return row;
}

/** Native invitation lookup by token hash, with fail-closed row validation. */
export async function nativeBetaInvitationByToken(
  db: SqlDatabase,
  token: string,
): Promise<InvitationRow | null> {
  if (!identifier(token)) return fail();
  const row = await nativeManageOwnerReadFirst<InvitationRow>(
    ME_RD_INVITATION_BY_TOKEN,
    [token],
    db,
  );
  if (row === null) return null;
  if (
    !identifier(row.id) ||
    typeof row.email !== 'string' ||
    row.email.length === 0 ||
    row.email.length > 320 ||
    !identifier(row.branch_id) ||
    !identifier(row.invited_by) ||
    !Number.isSafeInteger(row.expires_at) ||
    (row.accepted_user_id !== null && !identifier(row.accepted_user_id)) ||
    (row.revoked_at !== null && !Number.isSafeInteger(row.revoked_at)) ||
    !['employee', 'it_admin'].includes(row.role)
  )
    return fail();
  return row;
}

/** Native accepted-user lookup by invitation id. */
export async function nativeBetaInvitationAcceptedUser(
  db: SqlDatabase,
  id: string,
): Promise<{ accepted_user_id: string } | null> {
  if (!identifier(id)) return fail();
  const row = await nativeManageOwnerReadFirst<{ accepted_user_id: string }>(
    ME_RD_INVITATION_ACCEPTED_USER,
    [id],
    db,
  );
  if (row === null) return null;
  if (!identifier(row.accepted_user_id)) return fail();
  return row;
}

/** Native Company provisioning attachment-source image. The owner plans the
 * byte-identical statement; the caller binds the target id and Company id. */
export async function nativeCompanyProvisioningAttachmentImage(
  db: SqlDatabase,
  id: string,
  companyId: string,
): Promise<string | null> {
  if (!identifier(id) || !identifier(companyId)) return fail();
  const statement = await plan(ME_RD_COMPANY_PROVISIONING_ATTACHMENT_IMAGE, [id, companyId]);
  const image = await db.prepare(statement.sql).bind(...statement.values).first<string>('image');
  if (image === null) return null;
  if (typeof image !== 'string' || image.length === 0) return fail();
  return image;
}
