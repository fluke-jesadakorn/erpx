import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: ReadonlyArray<Readonly<{ origin: string; holdingSlug: string; holdingId: string }>>;
}>;

type Statement = Readonly<{ sql: string; values: unknown[] }>;

const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_COLLABORATION_CONFIG;
  if (!raw) throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  const invalid = (): never => {
    throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.');
  };
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return invalid();
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return invalid();
  const config = parsed as Record<string, unknown>;
  if (
    typeof config.privateBaseUrl !== 'string' ||
    typeof config.timeoutMs !== 'number' ||
    !Number.isSafeInteger(config.timeoutMs) ||
    config.timeoutMs < 250 ||
    config.timeoutMs > 30_000 ||
    !Array.isArray(config.holdings)
  )
    return invalid();
  let base: URL;
  try {
    base = new URL(config.privateBaseUrl);
  } catch {
    return invalid();
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
    config.privateBaseUrl !== base.origin
  )
    return invalid();
  const holdings: { origin: string; holdingSlug: string; holdingId: string }[] = [];
  for (const item of config.holdings) {
    if (item === null || typeof item !== 'object' || Array.isArray(item)) return invalid();
    const holding = item as Record<string, unknown>;
    if (
      typeof holding.origin !== 'string' ||
      !holding.origin ||
      typeof holding.holdingSlug !== 'string' ||
      !holding.holdingSlug ||
      typeof holding.holdingId !== 'string' ||
      !holding.holdingId
    )
      return invalid();
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return invalid();
    }
    if (origin.protocol !== 'https:' || origin.username || origin.password) return invalid();
    holdings.push({ origin: holding.origin, holdingSlug: holding.holdingSlug, holdingId: holding.holdingId });
  }
  return { privateBaseUrl: config.privateBaseUrl, timeoutMs: config.timeoutMs, holdings };
}

/** Ask the native Collaboration owner for the authoritative notification INSERT.
 * The caller executes it inside its own serializable unit. */
export async function nativeNotificationStatement(
  db: SqlDatabase,
  actor: WorkContext,
  input: {
    companyId: string;
    userId: string;
    recordType: string;
    recordId: string;
    recordVersion: number;
    message: string;
    kind?: string;
    leadId?: string;
    identifier?: string;
    approver_user_id?: string;
    item_id?: string;
    bill_id?: string;
    rfq_id?: string;
    audience?: string;
    event?: string;
    order_id?: string;
    change?: string;
    timestamp?: number;
    post_id?: string;
    source_hash?: string;
    ai_draft_id?: string;
    document_table?: string;
    recipient_sql?: string;
    node_id?: string;
    scope_node_id?: string;
    runtime_predicate?: string;
    table_name?: string;
    cte_prefix?: string;
    source_access_sql?: string;
    target_kind?: string;
    target_id?: string;
    /** camelCase aliases for kind-routed reads (the ingress denies unknown fields). */
    targetKind?: string;
    targetId?: string;
  },
  key: string,
  now: number,
): Promise<SqlStatement> {
  const config = configuration();
  const target = config.holdings.find(
    (holding) => holding.holdingSlug === actor.holdingSlug && holding.holdingId === actor.holdingId,
  );
  if (!target) throw new ApiError(503, 'NATIVE_COLLABORATION_AUTHORITY_INVALID', 'The native Collaboration authority is unavailable.');
  const headers = new Headers({
    host: new URL(target.origin).host,
    origin: target.origin,
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority?.revision ?? 0),
    'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion ?? 0),
  });
  if (actor.branchId) headers.set('x-erpx-workspace-branch', actor.branchId);
  if (actor.departmentId) headers.set('x-erpx-workspace-department', actor.departmentId);
  const requestBody = JSON.stringify({ ...input, now, idempotencyKey: key });
  let response: Response;
  try {
    response = await nativeRequest(
      new URL(
        `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/collaboration/notification-statement`,
        config.privateBaseUrl,
      ),
      {
        method: 'POST',
        headers: Object.fromEntries(headers),
        body: requestBody,
        signal: AbortSignal.timeout(config.timeoutMs),
      },
      config.timeoutMs,
    );
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  }
  const body = (await response.json().catch(() => null)) as
    | { data?: Statement; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok || !body?.data)
    throw new ApiError(response.status, body?.error?.code ?? 'NATIVE_COLLABORATION_FAILED', body?.error?.message ?? 'The Collaboration request failed.');
  return db.prepare(body.data.sql).bind(...body.data.values);
}
