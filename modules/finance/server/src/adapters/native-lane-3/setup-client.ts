import "server-only";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { nativeRequest } from '@erpx/platform-runtime-server/public/native-http';
import { lane3Config, type Lane3Actor } from "./client";

/**
 * Strict private transport for the native Finance lane 3 setup-counts read.
 *
 * The axum owner executes the five company-scoped statements and projects the
 * result. No SQL is authored or accepted here. The Holding is selected only by
 * the caller's exact WorkContext pair; an unknown pair fails closed.
 */
export type SetupCounts = Readonly<{
  accountCount: number;
  periodCount: number;
  taxCount: number;
  bankCount: number;
  firstCurrency: string | null;
}>;

const fail = (status = 503): never => {
  throw new ApiError(status, "LANE3_SETUP_COUNTS_UNAVAILABLE", "The native Finance setup-counts service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const id = (value: unknown): value is string =>
  typeof value === "string" && value.length <= 200 && /^[A-Za-z0-9_.:-]+$/.test(value);
const slug = (value: unknown): value is string =>
  typeof value === "string" && /^[a-z0-9_-]{1,128}$/.test(value);
const count = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0;

/** Read the exact setup reference counts for one bounded Company. */
export async function nativeSetupCounts(actor: Lane3Actor): Promise<SetupCounts> {
  if (!object(actor) || !id(actor.holdingId) || !slug(actor.holdingSlug) || !id(actor.companyId))
    throw new ApiError(422, "LANE3_SETUP_COUNTS_ACTOR_INVALID", "Use one bounded WorkContext.");
  const config = lane3Config();
  const holding = config.holdings.find(
    (h) => h.holdingId === actor.holdingId && h.holdingSlug === actor.holdingSlug,
  );
  if (!holding)
    throw new ApiError(503, "LANE3_SETUP_COUNTS_TARGET_UNAVAILABLE", "The Holding context is not registered.");
  if (holding.companyId !== undefined && holding.companyId !== actor.companyId)
    throw new ApiError(503, "LANE3_SETUP_COUNTS_COMPANY_SCOPE_CHANGED", "The Holding company scope changed.");

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/finance/lane3/setup-counts`,
    config.base,
  );
  let response: Response;
  try {
    response = await nativeRequest(
      url,
      {
        method: "POST",
        headers: {
          host: holding.host,
          "x-erpx-native-host": holding.host,
          origin: holding.origin,
          accept: "application/json",
          "cache-control": "no-cache",
          "x-erpx-context-holding": holding.holdingId,
          "x-erpx-context-company": actor.companyId,
        },
        body: "",
        signal: AbortSignal.timeout(config.timeoutMs),
      },
      config.timeoutMs,
    );
  } catch {
    return fail();
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    return fail();
  }
  let payload: unknown;
  try {
    payload = await response.json();
  } catch {
    throw new ApiError(502, "LANE3_SETUP_COUNTS_RESPONSE_INVALID", "The native response is invalid.");
  }
  if (!object(payload) || !object(payload.data) || !object(payload.data.setup)) {
    throw new ApiError(502, "LANE3_SETUP_COUNTS_RESPONSE_INVALID", "The native response is invalid.");
  }
  const setup = payload.data.setup;
  if (
    !count(setup.accountCount) ||
    !count(setup.periodCount) ||
    !count(setup.taxCount) ||
    !count(setup.bankCount) ||
    !(setup.firstCurrency === null ||
      (typeof setup.firstCurrency === "string" && /^[A-Z]{3}$/.test(setup.firstCurrency)))
  ) {
    throw new ApiError(502, "LANE3_SETUP_COUNTS_RESPONSE_INVALID", "The native response is invalid.");
  }
  return {
    accountCount: setup.accountCount,
    periodCount: setup.periodCount,
    taxCount: setup.taxCount,
    bankCount: setup.bankCount,
    firstCurrency: setup.firstCurrency as string | null,
  };
}
