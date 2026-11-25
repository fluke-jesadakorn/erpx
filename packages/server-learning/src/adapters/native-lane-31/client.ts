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

/** Read one owner-local evidence row from the native axum route. */
export async function nativeLearningEvidenceRow(
  actor: Pick<WorkContext, "holdingId" | "holdingSlug" | "companyId" | "branchId" | "departmentId">,
  kind: LearningEvidenceKind,
  id: string,
  range?: { from: number; toExclusive: number },
): Promise<NativeLearningEvidenceRow | null> {
  if (!text(id, 200) || (range && (!integer(range.from) || !integer(range.toExclusive) || range.from >= range.toExclusive)))
    throw new ApiError(422, "LEARNING_EVIDENCE_READ_REQUEST_INVALID", "Use one bounded Learning evidence read.");
  const config = configuration();
  // Select only the actor's registered Holding. There is no default: an unknown
  // (holdingId, holdingSlug) pair fails closed instead of using holdings[0].
  const target = config.holdings.find(
    (holding) => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (!target) return unavailable("LEARNING_EVIDENCE_READ_TARGET_UNAVAILABLE");
  if (target.companyId !== undefined && target.companyId !== actor.companyId)
    return unavailable("LEARNING_EVIDENCE_READ_TARGET_CHANGED");

  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "content-type": "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": actor.companyId,
  });
  if (text(actor.branchId, 200)) headers.set("x-erpx-context-branch", actor.branchId);
  if (text(actor.departmentId, 200)) headers.set("x-erpx-context-department", actor.departmentId);

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

/** True when the native consumer switch is configured. */
export function nativeLearningEvidenceConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
