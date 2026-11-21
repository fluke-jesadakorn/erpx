import "server-only";
import { headers } from "next/headers";
import { ApiError } from "@erpx/server-runtime/public/http";

/**
 * Native Billing owner-write transport. Every remaining owner SQL literal is
 * owned by the Rust crate and selected by an allow-listed `kind`; this client
 * only carries the kind, positional values and bounded transport facts. The
 * caller binds and executes the returned statement in its own transaction,
 * together with its receipt/audit/outbox effects. A missing or malformed owner
 * response fails closed - there is no TypeScript SQL fallback or mirror.
 *
 * The owner route matches an exact registered Holding target from Host/Origin,
 * so callers need only send kind + values; the target Holding is routing, while
 * the real holding id is carried inside the owner-authored SQL values.
 */
export type BillingOwnerStatement = Readonly<{ sql: string; values: unknown[] }>;

type Holding = Readonly<{
  origin: string;
  holdingSlug: string;
  holdingId: string;
}>;
type Settings = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Holding[];
  timeoutMs: number;
}>;

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_BILLING_CONFIG;
  if (raw === undefined)
    throw new ApiError(
      503,
      "NATIVE_BILLING_UNAVAILABLE",
      "The native Billing owner service is not configured.",
    );
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (
      !value ||
      typeof value.privateBaseUrl !== "string" ||
      !Array.isArray(value.holdings) ||
      value.holdings.length === 0 ||
      typeof value.timeoutMs !== "number"
    )
      throw new Error("invalid");
    return value as unknown as Settings;
  } catch {
    throw new ApiError(
      503,
      "NATIVE_BILLING_CONFIGURATION_INVALID",
      "Billing owner transport is misconfigured.",
    );
  }
}

async function incoming(): Promise<Headers | undefined> {
  try {
    return await headers();
  } catch {
    return undefined;
  }
}

/** Fetch the owner-authored statement for one typed Billing write. */
export async function nativeBillingOwnerStatement(
  kind: string,
  values: readonly unknown[],
  now: number = Date.now(),
): Promise<BillingOwnerStatement> {
  const config = configuration();
  const request = await incoming();
  const requestHost = request?.get("host");
  const target =
    config.holdings.find((holding) => new URL(holding.origin).host === requestHost) ??
    config.holdings[0]!;
  const sent = new Headers({
    host: new URL(target.origin).host,
    origin: target.origin,
    "content-type": "application/json",
    "idempotency-key": `billing:owner:${kind}:${now}`,
  });
  const cookie = request?.get("cookie");
  if (cookie) {
    sent.set("cookie", cookie);
    for (const name of [
      "x-erpx-platform-assignment",
      "x-erpx-context-revision",
      "x-erpx-context-company",
      "x-erpx-context-assignment",
      "x-erpx-context-assignment-version",
    ]) {
      const value = request?.get(name);
      if (value !== null && value !== undefined) sent.set(name, value);
    }
  } else {
    // Workers and jobs have no browser session; they are bound by the exact
    // registered Holding target and the bounded Idempotency-Key.
    sent.set("x-erpx-owner-write-system", "1");
  }
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/billing/owner-write`,
      {
        method: "POST",
        headers: sent,
        body: JSON.stringify({ kind, values, now }),
        cache: "no-store",
        redirect: "manual",
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    throw new ApiError(
      503,
      "NATIVE_BILLING_UNAVAILABLE",
      "The native Billing owner service is unavailable.",
    );
  }
  const payload = (await response.json().catch(() => null)) as {
    data?: BillingOwnerStatement;
    error?: { code?: string; message?: string };
  } | null;
  if (!response.ok || !payload?.data)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? "NATIVE_BILLING_OWNER_WRITE_FAILED",
      payload?.error?.message ?? "The native Billing owner write failed.",
    );
  const statement = payload.data;
  if (
    typeof statement.sql !== "string" ||
    statement.sql.length === 0 ||
    !Array.isArray(statement.values) ||
    statement.values.length === 0
  )
    throw new ApiError(
      503,
      "NATIVE_BILLING_RESPONSE_INVALID",
      "The native Billing owner statement was invalid.",
    );
  return { sql: statement.sql, values: statement.values };
}

/** pg PoolClient-style caller: fetch the owner SQL, execute in the caller's unit. */
export async function nativeBillingOwnerQuery(
  client: { query: (sql: string, values: readonly unknown[]) => Promise<unknown> },
  kind: string,
  values: readonly unknown[],
  now: number = Date.now(),
): Promise<any> {
  const statement = await nativeBillingOwnerStatement(kind, values, now);
  return client.query(statement.sql, statement.values);
}

/** SqlDatabase-style caller: fetch the owner SQL and bind it to a statement. */
export async function nativeBillingOwnerBind(
  db: {
    prepare: (
      sql: string,
    ) => { bind: (...values: unknown[]) => unknown };
  },
  kind: string,
  values: readonly unknown[],
  now: number = Date.now(),
): Promise<any> {
  const statement = await nativeBillingOwnerStatement(kind, values, now);
  return db.prepare(statement.sql).bind(...statement.values);
}
