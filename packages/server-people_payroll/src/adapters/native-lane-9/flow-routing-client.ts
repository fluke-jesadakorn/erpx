import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {
  configuration,
  direct,
  selectTarget,
  type Lane9Actor,
  type Transport,
} from "./client";

/**
 * Strict private transport for the lane-9 payroll payment-batch reads. The axum
 * owner executes both Company-scoped statements with sqlx and owns the
 * projection and response shape; this bridge emits no SQL and accepts no SQL
 * from a caller. Every registered Holding is selected explicitly (WorkContext
 * `(holdingId, holdingSlug)`, otherwise an exact configured `companyId`); there
 * is no first-holding default and any unknown target fails closed.
 */
const ENV = "ERPX_NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG";
const MAX_BATCHES = 10_000;

/** SHA-256 of the owner Rust `SQL_PAYMENT_BATCHES` constant. */
export const SQL_LIST_ORACLE_SHA256 =
  "567f9caef077f1fa5ff662a809bdca5e31e9a992b45ebbe583c3590d9bccf6f5";
/** SHA-256 of the owner Rust `SQL_PAYMENT_BATCH_BY_SOURCE` constant. */
export const SQL_DETAIL_ORACLE_SHA256 =
  "fb3b7bf6130a99a6c91cea4eb88390885b4a70b8b50ecadffffca51d42d63220";
/** SHA-256 of the pinned list / point native response payloads. */
export const LIST_RESPONSE_ORACLE_SHA256 =
  "0b8fcb3eb183bb535b183dcd6022793405fee2f790d497f2aada8797da45f1f5";
export const DETAIL_RESPONSE_ORACLE_SHA256 =
  "46c9321269d025f43d575b0436f1bc4101166064b6761b9bd17b1c627f8a9583";

const fail = (code: string, status = 503): never => {
  throw new ApiError(status, code, "The native People Payroll payment-batch service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const validId = (value: unknown): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= 200 && /^[A-Za-z0-9._:-]+$/.test(value);
const bounded = (value: unknown): value is string => validId(value);

function exactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  return Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
}

async function call(
  path: string,
  companyId: string,
  sourceId: string | null,
  actor: Lane9Actor | undefined,
  transport: Transport,
): Promise<unknown> {
  if (!validId(companyId)) return fail("PEOPLE_PAYROLL_LANE9_FLOW_COMPANY_INVALID", 422);
  if (sourceId !== null && !validId(sourceId)) return fail("PEOPLE_PAYROLL_LANE9_FLOW_SOURCE_INVALID", 422);
  const config = configuration(process.env[ENV]);
  const target = selectTarget(config, actor, companyId);
  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": companyId,
    ...(sourceId === null ? {} : { "x-erpx-context-source": sourceId }),
  });
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}${path}`, config.base);
  let response: Response;
  try {
    response = await transport(url, headers, AbortSignal.timeout(config.timeoutMs));
  } catch {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_FLOW_UNAVAILABLE");
  }
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_FLOW_REDIRECT");
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    if (response.status >= 400 && response.status < 500) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE9_FLOW_REJECTED", response.status);
    }
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_FLOW_UNAVAILABLE");
  }
  return response.json().catch(() => null);
}

type ListRow = Readonly<{ source_id: string; id: string; status: string; created_by: string }>;
type DetailRow = Readonly<{ id: string; status: string; created_by: string }>;

/** Native replacement of `readWorkflowPaymentBatches`. `database` is retained
 * only for call-site compatibility; no statement is issued on it. */
export async function paymentBatches(
  _database: SqlDatabase,
  companyId: string,
  actor?: Lane9Actor,
  transport: Transport = direct,
): Promise<ListRow[]> {
  const payload: unknown = await call(
    "/people-payroll/payment-batches",
    companyId,
    null,
    actor,
    transport,
  );
  if (!object(payload) || Object.keys(payload).length !== 1 || !Array.isArray(payload.batches)) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_FLOW_RESPONSE_INVALID", 502);
  }
  if (payload.batches.length > MAX_BATCHES) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_FLOW_RESPONSE_INVALID", 502);
  }
  return payload.batches.map((item: unknown) => {
    if (!object(item) || !exactKeys(item, ["source_id", "id", "status", "created_by"])) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE9_FLOW_RESPONSE_INVALID", 502);
    }
    if (!bounded(item.source_id) || !bounded(item.id) || !bounded(item.status) || !bounded(item.created_by)) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE9_FLOW_RESPONSE_INVALID", 502);
    }
    return {
      source_id: item.source_id,
      id: item.id,
      status: item.status,
      created_by: item.created_by,
    };
  });
}

/** Native replacement of `readDefinitionsPaymentBatch`. A null source identity
 * yields no row without issuing a request (the original `source_id=?` with NULL
 * matched nothing); no TypeScript SQL fallback exists. */
export async function paymentBatchBySource(
  _database: SqlDatabase,
  companyId: string,
  sourceId: string | number | null,
  actor?: Lane9Actor,
  transport: Transport = direct,
): Promise<DetailRow | undefined> {
  const source = sourceId === null || sourceId === undefined ? null : String(sourceId);
  if (source !== null && !validId(source)) return fail("PEOPLE_PAYROLL_LANE9_FLOW_SOURCE_INVALID", 422);
  if (source === null) return undefined;
  const payload: unknown = await call(
    "/people-payroll/payment-batch",
    companyId,
    source,
    actor,
    transport,
  );
  if (!object(payload) || Object.keys(payload).length !== 1 || !Object.hasOwn(payload, "batch")) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_FLOW_RESPONSE_INVALID", 502);
  }
  const batch: unknown = payload.batch;
  if (batch === null || batch === undefined) return undefined;
  if (!object(batch) || !exactKeys(batch, ["id", "status", "created_by"])) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_FLOW_RESPONSE_INVALID", 502);
  }
  if (!bounded(batch.id) || !bounded(batch.status) || !bounded(batch.created_by)) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_FLOW_RESPONSE_INVALID", 502);
  }
  return { id: batch.id, status: batch.status, created_by: batch.created_by };
}
