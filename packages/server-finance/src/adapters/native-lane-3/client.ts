import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import { nativeRequest } from '@erpx/server-runtime/public/native-http';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { PersistedInvoiceLine } from "@erpx/server-finance/domain/invoice-line-revision";

/**
 * Strict private transport for the native Finance lane 3 invoice-line read.
 *
 * The axum owner executes the SQL, owns the company scope and returns the full
 * projected row shape. This bridge emits no SQL, accepts no caller-supplied
 * statement and validates every returned field. A Holding is selected only by
 * the caller's exact WorkContext pair; there is no first-holding default. A
 * missing configuration, unknown target or malformed reply fails closed.
 */
export type Lane3Actor = Pick<WorkContext, "holdingId" | "holdingSlug" | "companyId">;

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;

export const LANE3_CONFIG_KEY = "ERPX_NATIVE_LANE3_READ_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const ORIGIN_KINDS = new Set(["historical_header", "whole_document", "native_line"]);

/** The native row also carries its company scope as owner evidence. */
export type NativeInvoiceLine = PersistedInvoiceLine & Readonly<{ company_id: string }>;

const unavailable = (code = "LANE3_INVOICE_LINES_UNAVAILABLE", status = 503): never => {
  throw new ApiError(status, code, "The native Finance invoice-line service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const record = (value: unknown): value is Record<string, unknown> => object(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && value.length <= max && !/[\u0000-\u001f]/.test(value);
const id = (value: unknown): value is string =>
  text(value, 200) && /^[A-Za-z0-9_.:-]+$/.test(value as string);
const slug = (value: unknown): value is string =>
  typeof value === "string" && /^[a-z0-9_-]{1,128}$/.test(value);
const integer = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value);
const nullableInteger = (value: unknown): value is number | null => value === null || integer(value);

export function lane3Config(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): Config {
  const raw = environment[LANE3_CONFIG_KEY];
  if (raw === undefined) return unavailable("LANE3_INVOICE_LINES_CONFIGURATION_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("LANE3_INVOICE_LINES_CONFIGURATION_INVALID");
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
    return unavailable("LANE3_INVOICE_LINES_CONFIGURATION_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable("LANE3_INVOICE_LINES_CONFIGURATION_INVALID");
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
    return unavailable("LANE3_INVOICE_LINES_CONFIGURATION_INVALID");

  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !id(item.holdingId) ||
      !slug(item.holdingSlug) ||
      (item.companyId !== undefined && !id(item.companyId))
    )
      return unavailable("LANE3_INVOICE_LINES_CONFIGURATION_INVALID");
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable("LANE3_INVOICE_LINES_CONFIGURATION_INVALID");
    }
    if (origin.protocol !== "https:" || origin.origin !== item.origin || origin.username || origin.password)
      return unavailable("LANE3_INVOICE_LINES_CONFIGURATION_INVALID");
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  if (
    new Set(holdings.map((h) => `${h.holdingId}\u0000${h.holdingSlug}`)).size !== holdings.length ||
    new Set(holdings.map((h) => `${h.host}\u0000${h.holdingSlug}`)).size !== holdings.length
  )
    return unavailable("LANE3_INVOICE_LINES_CONFIGURATION_INVALID");
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Resolve the exact registered target from the caller's WorkContext. */
function resolve(actor: Lane3Actor, config: Config): { holding: Holding; companyId: string } {
  if (!record(actor) || !id(actor.holdingId) || !slug(actor.holdingSlug) || !id(actor.companyId))
    return unavailable("LANE3_INVOICE_LINES_ACTOR_INVALID");
  const holding = config.holdings.find(
    (h) => h.holdingId === actor.holdingId && h.holdingSlug === actor.holdingSlug,
  );
  if (!holding) return unavailable("LANE3_INVOICE_LINES_TARGET_UNAVAILABLE");
  if (holding.companyId !== undefined && holding.companyId !== actor.companyId)
    return unavailable("LANE3_INVOICE_LINES_COMPANY_SCOPE_CHANGED");
  return { holding, companyId: actor.companyId };
}

function line(value: unknown, companyId: string, invoiceId: string, revision: number): NativeInvoiceLine {
  if (!object(value)) return unavailable("LANE3_INVOICE_LINES_RESPONSE_INVALID", 502);
  if (
    !id(value.id) ||
    value.invoice_id !== invoiceId ||
    !id(value.company_id) ||
    value.company_id !== companyId ||
    value.source_version !== revision ||
    !integer(value.ordinal) ||
    value.ordinal < 0 ||
    !text(value.description) ||
    !nullableInteger(value.quantity_millis) ||
    !nullableInteger(value.unit_price_minor) ||
    !nullableInteger(value.tax_bps) ||
    !integer(value.subtotal_minor) ||
    !integer(value.tax_minor) ||
    !integer(value.total_minor) ||
    value.total_minor !== (value.subtotal_minor as number) + (value.tax_minor as number) ||
    !id(value.account_id) ||
    !(value.tax_account_id === null || id(value.tax_account_id)) ||
    !text(value.source_snapshot_json, 1_048_576) ||
    !text(value.currency, 3) ||
    !/^[A-Z]{3}$/.test(value.currency as string) ||
    !text(value.origin_kind, 40) ||
    !ORIGIN_KINDS.has(value.origin_kind as string)
  )
    return unavailable("LANE3_INVOICE_LINES_RESPONSE_INVALID", 502);
  return {
    id: value.id as string,
    company_id: companyId,
    invoice_id: invoiceId,
    source_version: revision,
    ordinal: value.ordinal as number,
    description: value.description as string,
    quantity_millis: value.quantity_millis as number | null,
    unit_price_minor: value.unit_price_minor as number | null,
    tax_bps: value.tax_bps as number | null,
    subtotal_minor: value.subtotal_minor as number,
    tax_minor: value.tax_minor as number,
    total_minor: value.total_minor as number,
    account_id: value.account_id as string,
    tax_account_id: value.tax_account_id as string | null,
    source_snapshot_json: value.source_snapshot_json as string,
    currency: value.currency as string,
    origin_kind: value.origin_kind as string,
  };
}

/** Read the exact invoice-line revision from the native owner. No SQL is emitted. */
export async function nativeCurrentInvoiceLines(
  actor: Lane3Actor,
  invoiceId: string,
  revision: number,
): Promise<NativeInvoiceLine[]> {
  if (!id(invoiceId) || !integer(revision) || revision <= 0)
    return unavailable("LANE3_INVOICE_LINES_INPUT_INVALID", 422);
  const config = lane3Config();
  const { holding, companyId } = resolve(actor, config);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/finance/lane3/invoice-lines`,
    config.base,
  );
  let response: Response;
  try {
    response = await nativeRequest(
      url,
      {
        method: "GET",
        headers: {
          host: holding.host,
          "x-erpx-native-host": holding.host,
          origin: holding.origin,
          accept: "application/json",
          "content-type": "application/json",
          "cache-control": "no-cache",
          "x-erpx-context-holding": holding.holdingId,
          "x-erpx-context-company": companyId,
        },
        body: JSON.stringify({ invoiceId, revision }),
        signal: AbortSignal.timeout(config.timeoutMs),
      },
      config.timeoutMs,
    );
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  let payload: unknown;
  try {
    payload = await response.json();
  } catch {
    return unavailable("LANE3_INVOICE_LINES_RESPONSE_INVALID", 502);
  }
  if (!object(payload) || !object(payload.data) || !Array.isArray(payload.data.lines))
    return unavailable("LANE3_INVOICE_LINES_RESPONSE_INVALID", 502);
  return payload.data.lines.map((row: unknown) => line(row, companyId, invoiceId, revision));
}

/** True when the lane 3 native consumer switch is configured. */
export function nativeLane3Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[LANE3_CONFIG_KEY] !== undefined;
}
