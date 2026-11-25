import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/**
 * Strict private transport for the owner-local Learning evidence-detail read.
 *
 * Rust owns the SQL, the projection, the company/branch/department scope and
 * the response shape. This bridge emits no SQL, accepts no SQL from a caller,
 * and only transports bounded scalar identities. Missing configuration or any
 * rejected/invalid native reply fails closed with no TypeScript fallback.
 */
export type LearningEvidenceKind = "learning-completions" | "overdue-required-learning";
export type NativeLearningEvidenceRow = {
  id: string;
  assignment_id?: string;
  event_version?: number;
  version?: number;
  status: string;
  created_at?: number;
  due_date?: string;
  updated_at?: number;
  course_title: string;
  required_for?: string | null;
};

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Settings = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

const ENV = "ERPX_NATIVE_LEARNING_EVIDENCE_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (code = "LEARNING_EVIDENCE_READ_UNAVAILABLE"): never => {
  throw new ApiError(503, code, "The native Learning evidence read is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;
const integer = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value);

function configuration(): Settings {
  const raw = process.env[ENV];
  if (!raw) return unavailable("LEARNING_EVIDENCE_READ_CONFIGURATION_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("LEARNING_EVIDENCE_READ_CONFIGURATION_INVALID");
  }
  if (
    !object(value) ||
    !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== "number" ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("LEARNING_EVIDENCE_READ_CONFIGURATION_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable("LEARNING_EVIDENCE_READ_CONFIGURATION_INVALID");
  }
  const port = Number(base.port);
  if (
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  )
    return unavailable("LEARNING_EVIDENCE_READ_CONFIGURATION_INVALID");
  const holdings: Holding[] = [];
  const registered = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 253) ||
      !text(item.holdingId, 200) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(String(item.holdingSlug)) ||
      (item.companyId !== undefined && !text(item.companyId, 200))
    )
      return unavailable("LEARNING_EVIDENCE_READ_CONFIGURATION_INVALID");
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable("LEARNING_EVIDENCE_READ_CONFIGURATION_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      // The Host header must be the exact host the Rust router registered for
      // this Holding; a mismatch would route the read to the wrong pool.
      origin.host !== item.host
    )
      return unavailable("LEARNING_EVIDENCE_READ_CONFIGURATION_INVALID");
    const key = `${String(item.host)}\u0000${String(item.holdingSlug)}\u0000${String(item.holdingId)}`;
    if (registered.has(key)) return unavailable("LEARNING_EVIDENCE_READ_CONFIGURATION_INVALID");
    registered.add(key);
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Strictly validate one projected row; unknown keys are rejected so the
 * response shape cannot drift away from the Rust owner. */
function row(value: unknown): NativeLearningEvidenceRow | null {
  if (value === null) return null;
  if (!object(value)) return unavailable("LEARNING_EVIDENCE_READ_RESPONSE_INVALID");
  const allowed = new Set([
    "id",
    "assignment_id",
    "event_version",
    "version",
    "status",
    "created_at",
    "due_date",
    "updated_at",
    "course_title",
    "required_for",
  ]);
  for (const key of Object.keys(value)) if (!allowed.has(key)) return unavailable("LEARNING_EVIDENCE_READ_RESPONSE_INVALID");
  if (!text(value.id, 200) || !text(value.status, 200) || !text(value.course_title, 8192))
    return unavailable("LEARNING_EVIDENCE_READ_RESPONSE_INVALID");
  const optionalText = (key: string, max: number) => {
    const v = value[key];
    return v === undefined || v === null || text(v, max) ? (v as string | null | undefined) : unavailable("LEARNING_EVIDENCE_READ_RESPONSE_INVALID");
  };
  const optionalInt = (key: string) => {
    const v = value[key];
    return v === undefined || v === null || integer(v) ? (v as number | undefined) : unavailable("LEARNING_EVIDENCE_READ_RESPONSE_INVALID");
  };
  return {
    id: value.id,
    status: value.status,
    course_title: value.course_title,
    assignment_id: optionalText("assignment_id", 200) as string | undefined,
    event_version: optionalInt("event_version"),
    version: optionalInt("version"),
    created_at: optionalInt("created_at"),
    due_date: optionalText("due_date", 200) as string | undefined,
    updated_at: optionalInt("updated_at"),
    required_for: optionalText("required_for", 200) as string | null | undefined,
  };
}

/** The actor fields that select a registered Holding and its company scope. */
export type LearningEvidenceActor = Pick<
  WorkContext,
  "holdingId" | "holdingSlug" | "companyId" | "branchId" | "departmentId"
>;

export type LearningEvidenceSearchInput = Readonly<{
  metric: LearningEvidenceKind;
  query: string;
  filters: Readonly<{ from: string; to: string; currency: string }>;
  limit: number;
}>;

export type LearningEvidenceCursor =
  | Readonly<{ createdAt: number; id: string }>
  | Readonly<{ dueDate: string; id: string }>
  | null;

/** Select the actor's exact registered Holding. Never falls back to holdings[0]. */
function selectTarget(actor: LearningEvidenceActor) {
  const config = configuration();
  const target = config.holdings.find(
    (holding) => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (!target) return unavailable("LEARNING_EVIDENCE_READ_TARGET_UNAVAILABLE");
  if (target.companyId !== undefined && target.companyId !== actor.companyId)
    return unavailable("LEARNING_EVIDENCE_READ_TARGET_CHANGED");
  return { config, target };
}

function contextHeaders(actor: LearningEvidenceActor, host: string, origin: string) {
  const headers = new Headers({
    host,
    origin,
    accept: "application/json",
    "content-type": "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": actor.companyId,
  });
  if (text(actor.branchId, 200)) headers.set("x-erpx-context-branch", actor.branchId);
  if (text(actor.departmentId, 200)) headers.set("x-erpx-context-department", actor.departmentId);
  return headers;
}

/** Read one owner-local evidence row from the native axum route. */
export async function nativeLearningEvidenceRow(
  actor: LearningEvidenceActor,
  kind: LearningEvidenceKind,
  id: string,
  range?: { from: number; toExclusive: number },
): Promise<NativeLearningEvidenceRow | null> {
  if (!text(id, 200) || (range && (!integer(range.from) || !integer(range.toExclusive) || range.from >= range.toExclusive)))
    throw new ApiError(422, "LEARNING_EVIDENCE_READ_REQUEST_INVALID", "Use one bounded Learning evidence read.");
  const { config, target } = selectTarget(actor);
  const headers = contextHeaders(actor, target.host, target.origin);

  const body = JSON.stringify({
    kind,
    id,
    from: range?.from ?? null,
    toExclusive: range?.toExclusive ?? null,
  });
  if (new TextEncoder().encode(body).length > 16 * 1024)
    throw new ApiError(422, "LEARNING_EVIDENCE_READ_REQUEST_INVALID", "Use one bounded Learning evidence read.");

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/learning/evidence-detail`,
    config.base,
  );
  let response: Response;
  try {
    response = await fetch(url, {
      method: "POST",
      headers: Object.fromEntries(headers),
      body,
      cache: "no-store",
      redirect: "error",
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    const payload = await response.json().catch((): unknown => null);
    void response.body?.cancel().catch(() => undefined);
    if (response.status >= 400 && response.status < 500) {
      const error = object(payload) && object(payload.error) ? payload.error : undefined;
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : "LEARNING_EVIDENCE_READ_FAILED",
        text(error?.message, 300) ? String(error!.message) : "The native Learning evidence read was rejected.",
      );
    }
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  if (!object(payload) || !("row" in payload)) return unavailable();
  return row(payload.row);
}

/** Search the owner-local evidence list on the native axum route. The caller
 * supplies only bounded scalar filters and an already-parsed cursor; the owner
 * renders the predicates, applies company scope, projects rows and validates
 * the reply. No SQL is emitted here and a rejected runtime fails closed. */
export async function nativeLearningEvidenceSearch(
  actor: LearningEvidenceActor,
  input: LearningEvidenceSearchInput,
  cursor: LearningEvidenceCursor,
): Promise<NativeLearningEvidenceRow[]> {
  const fromMs = Date.parse(input.filters.from);
  const toMs = Date.parse(input.filters.to);
  if (
    !integer(input.limit) ||
    input.limit < 1 ||
    input.limit > 50 ||
    !text(input.query, 100) ||
    !text(input.filters.from, 10) ||
    !text(input.filters.to, 10) ||
    !Number.isFinite(fromMs) ||
    !Number.isFinite(toMs) ||
    fromMs >= toMs
  )
    throw new ApiError(422, "LEARNING_EVIDENCE_READ_REQUEST_INVALID", "Use one bounded Learning evidence search.");
  if (cursor && (!text(cursor.id, 200) ||
    ("createdAt" in cursor ? !integer(cursor.createdAt) : !/^\d{4}-\d{2}-\d{2}$/.test(cursor.dueDate))))
    throw new ApiError(422, "LEARNING_EVIDENCE_READ_REQUEST_INVALID", "Use one bounded Learning evidence cursor.");

  const { config, target } = selectTarget(actor);
  const headers = contextHeaders(actor, target.host, target.origin);
  const completion = input.metric === "learning-completions";
  const body = JSON.stringify({
    kind: input.metric,
    query: input.query,
    from: completion ? Date.parse(input.filters.from) : null,
    toExclusive: completion ? Date.parse(input.filters.to) + 86_400_000 : null,
    limit: input.limit,
    cursor: cursor ?? null,
  });
  if (new TextEncoder().encode(body).length > 16 * 1024)
    throw new ApiError(422, "LEARNING_EVIDENCE_READ_REQUEST_INVALID", "Use one bounded Learning evidence search.");

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/learning/evidence-search`,
    config.base,
  );
  let response: Response;
  try {
    response = await fetch(url, {
      method: "POST",
      headers: Object.fromEntries(headers),
      body,
      cache: "no-store",
      redirect: "error",
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    const payload = await response.json().catch((): unknown => null);
    void response.body?.cancel().catch(() => undefined);
    if (response.status >= 400 && response.status < 500) {
      const error = object(payload) && object(payload.error) ? payload.error : undefined;
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : "LEARNING_EVIDENCE_READ_FAILED",
        text(error?.message, 300) ? String(error!.message) : "The native Learning evidence search was rejected.",
      );
    }
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  if (!object(payload) || !Array.isArray(payload.rows)) return unavailable();
  const rows: NativeLearningEvidenceRow[] = [];
  for (const value of payload.rows) {
    const parsed = row(value);
    if (parsed === null) return unavailable(); // a null row inside a list is invalid
    rows.push(parsed);
  }
  return rows;
}

/** True when the native consumer switch is configured. */
export function nativeLearningEvidenceConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
