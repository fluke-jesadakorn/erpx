import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/**
 * Strict transport for the native Sales customer-note read (lane 30). The axum
 * owner executes the statements, enforces the company/branch/department scope
 * and builds the `{notes,history}` response. Up to 64 registered Holdings are
 * accepted; the target is resolved from the caller's WorkContext and an optional
 * configured companyId pin. This bridge emits no SQL, accepts no SQL from a
 * caller, never defaults to the first Holding, and fails closed with a 503 when
 * the native owner is unavailable or returns anything other than the exact
 * pinned shape.
 */
export type NativeNoteActor = Pick<
  WorkContext,
  "holdingId" | "holdingSlug" | "companyId" | "branchId" | "departmentId"
>;
export type NativeNoteSource = "order" | "customer";
export type NativeNoteRow = Record<string, unknown>;
export type NativeCustomerNotes = Readonly<{ notes: NativeNoteRow[]; history: NativeNoteRow[] }>;

export type NativeNoteTransport = (
  url: URL,
  headers: Headers,
  signal: AbortSignal,
) => Promise<Response>;

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

export const NATIVE_SALES_NOTES_CONFIG_ENV = "ERPX_NATIVE_SALES_NOTES_CONFIG";
const ROUTE = "/api/v1/holdings/{holding_slug}/sales/customer-notes";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (code = "SALES_NATIVE_CUSTOMER_NOTES_UNAVAILABLE"): never => {
  throw new ApiError(503, code, "The native Sales customer-note read is unavailable.");
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 4096): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const slug = (value: unknown): value is string =>
  typeof value === "string" && value.length <= 128 && /^[a-z0-9_-]+$/.test(value);
const count = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 1;

function configuration(raw: string): Config {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("SALES_NATIVE_CUSTOMER_NOTES_CONFIG_INVALID");
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl, 512) ||
    !count(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("SALES_NATIVE_CUSTOMER_NOTES_CONFIG_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl as string);
  } catch {
    return unavailable("SALES_NATIVE_CUSTOMER_NOTES_CONFIG_INVALID");
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
    return unavailable("SALES_NATIVE_CUSTOMER_NOTES_CONFIG_INVALID");
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !identifier(item.host) ||
      !identifier(item.holdingId) ||
      !slug(item.holdingSlug) ||
      (item.companyId !== undefined && !identifier(item.companyId))
    )
      return unavailable("SALES_NATIVE_CUSTOMER_NOTES_CONFIG_INVALID");
    let origin: URL;
    try {
      origin = new URL(item.origin as string);
    } catch {
      return unavailable("SALES_NATIVE_CUSTOMER_NOTES_CONFIG_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.port ||
      origin.username ||
      origin.password ||
      item.host !== origin.host
    )
      return unavailable("SALES_NATIVE_CUSTOMER_NOTES_CONFIG_INVALID");
    holdings.push({
      origin: origin.origin,
      host: item.host as string,
      holdingId: item.holdingId as string,
      holdingSlug: item.holdingSlug as string,
      ...(item.companyId === undefined ? {} : { companyId: item.companyId as string }),
    });
  }
  return {
    base: base.origin,
    timeoutMs: value.timeoutMs as number,
    holdings,
  };
}

const NOTE_KEYS = [
  "id",
  "company_id",
  "customer_id",
  "order_id",
  "channel",
  "occurred_at",
  "body",
  "status",
  "version",
  "created_by",
  "created_at",
  "updated_at",
] as const;
const HISTORY_KEYS = [
  "id",
  "note_id",
  "version",
  "action",
  "body",
  "reason",
  "actor_id",
  "created_at",
] as const;
const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));

function noteRow(value: unknown, actor: NativeNoteActor, id: string, source: NativeNoteSource): boolean {
  if (!exact(value, NOTE_KEYS) || !identifier(value.id) || value.company_id !== actor.companyId)
    return false;
  if (!identifier(value.customer_id) || !text(value.body, 4000)) return false;
  if (
    !["phone", "email", "meeting", "other"].includes(String(value.channel)) ||
    !["active", "archived"].includes(String(value.status))
  )
    return false;
  if (
    !count(value.occurred_at) ||
    !count(value.version) ||
    !identifier(value.created_by) ||
    !count(value.created_at) ||
    !count(value.updated_at)
  )
    return false;
  if (value.order_id !== null && !identifier(value.order_id)) return false;
  if (source === "order" && value.order_id !== id) return false;
  if (source === "customer" && value.id === "") return false;
  return true;
}

function historyRow(value: unknown): boolean {
  return (
    exact(value, HISTORY_KEYS) &&
    identifier(value.id) &&
    identifier(value.note_id) &&
    count(value.version) &&
    ["create", "correct", "archive"].includes(String(value.action)) &&
    text(value.body, 4000) &&
    text(value.reason, 4000) &&
    identifier(value.actor_id) &&
    count(value.created_at)
  );
}

function response(value: unknown, actor: NativeNoteActor, id: string, source: NativeNoteSource): NativeCustomerNotes {
  if (!exact(value, ["notes", "history"])) return unavailable("SALES_NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID");
  if (
    !Array.isArray(value.notes) ||
    value.notes.length > 500 ||
    !Array.isArray(value.history) ||
    value.history.length > 2000
  )
    return unavailable("SALES_NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID");
  if (
    !value.notes.every((row) => noteRow(row, actor, id, source)) ||
    !value.history.every((row) => historyRow(row))
  )
    return unavailable("SALES_NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID");
  const ids = new Set<string>();
  for (const row of value.notes as NativeNoteRow[]) {
    if (ids.has(row.id as string)) return unavailable("SALES_NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID");
    ids.add(row.id as string);
  }
  return value as unknown as NativeCustomerNotes;
}

const directFetch: NativeNoteTransport = (url, headers, signal) =>
  fetch(url, { method: "GET", headers, cache: "no-store", redirect: "error", signal });

/** Build the strict native client. `transport` is injectable for owner tests. */
export function createCustomerNotesClient(
  rawConfig: string,
  transport: NativeNoteTransport = directFetch,
) {
  const config = configuration(rawConfig);
  return {
    async read(
      actor: NativeNoteActor,
      source: NativeNoteSource,
      id: string,
    ): Promise<NativeCustomerNotes> {
      if (!identifier(id) || !slug(actor.holdingSlug) || !identifier(actor.companyId))
        return unavailable("SALES_NATIVE_CUSTOMER_NOTES_REQUEST_INVALID");
      // Target resolution is actor-driven only. There is no first-holding
      // fallback: an unregistered Holding or Company fails closed.
      const holding = config.holdings.find(
        (item) => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
      );
      if (!holding) return unavailable("SALES_NATIVE_CUSTOMER_NOTES_TARGET_INVALID");
      if (holding.companyId !== undefined && holding.companyId !== actor.companyId)
        return unavailable("SALES_NATIVE_CUSTOMER_NOTES_TARGET_INVALID");
      if (actor.branchId !== null && !identifier(actor.branchId))
        return unavailable("SALES_NATIVE_CUSTOMER_NOTES_REQUEST_INVALID");
      if (actor.departmentId !== null && !identifier(actor.departmentId))
        return unavailable("SALES_NATIVE_CUSTOMER_NOTES_REQUEST_INVALID");

      const headers = new Headers({
        host: holding.host,
        origin: holding.origin,
        accept: "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": actor.companyId,
        "x-erpx-note-source": source,
        "x-erpx-note-id": id,
      });
      if (actor.branchId !== null) headers.set("x-erpx-context-branch", actor.branchId);
      if (actor.departmentId !== null) headers.set("x-erpx-context-department", actor.departmentId);
      const url = new URL(
        ROUTE.replace("{holding_slug}", encodeURIComponent(holding.holdingSlug)),
        config.base,
      );
      let incoming: Response;
      try {
        incoming = await transport(url, headers, AbortSignal.timeout(config.timeoutMs));
      } catch {
        return unavailable();
      }
      if (incoming.redirected) {
        void incoming.body?.cancel().catch(() => undefined);
        return unavailable();
      }
      if (incoming.status !== 200) {
        void incoming.body?.cancel().catch(() => undefined);
        return unavailable();
      }
      if ((incoming.headers.get("content-type") ?? "").split(";")[0]?.trim() !== "application/json") {
        void incoming.body?.cancel().catch(() => undefined);
        return unavailable("SALES_NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID");
      }
      let value: unknown;
      try {
        value = await incoming.json();
      } catch {
        return unavailable("SALES_NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID");
      }
      return response(value, actor, id, source);
    },
  };
}

/** Configuration-backed client used by `adapters/notes.ts`. */
export async function readCustomerNotes(
  actor: NativeNoteActor,
  source: NativeNoteSource,
  id: string,
): Promise<NativeCustomerNotes> {
  const raw = process.env[NATIVE_SALES_NOTES_CONFIG_ENV];
  if (raw === undefined) return unavailable("SALES_NATIVE_CUSTOMER_NOTES_CONFIG_REQUIRED");
  return createCustomerNotesClient(raw).read(actor, source, id);
}

/** Whether the native Sales note consumer switch is configured. */
export function nativeCustomerNotesConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[NATIVE_SALES_NOTES_CONFIG_ENV] !== undefined;
}
