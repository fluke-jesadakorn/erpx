import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import { nativeRequest } from '@erpx/server-runtime/public/native-http';
import { lane3Config, type Lane3Actor } from "./client";

/**
 * Strict private transport for the native Finance lane 3 starter probes.
 *
 * The axum owner executes the two company-scoped existence statements and
 * projects the booleans. No SQL is authored or accepted here. The Holding is
 * selected only by the caller's exact WorkContext pair; unknown pairs fail
 * closed with no TypeScript fallback.
 */
export type StarterProbes = Readonly<{ existingChart: boolean; overlappingPeriod: boolean }>;

const fail = (status = 503): never => {
  throw new ApiError(status, "LANE3_STARTER_PROBES_UNAVAILABLE", "The native Finance starter service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const id = (value: unknown): value is string =>
  typeof value === "string" && value.length <= 200 && /^[A-Za-z0-9_.:-]+$/.test(value);
const slug = (value: unknown): value is string =>
  typeof value === "string" && /^[a-z0-9_-]{1,128}$/.test(value);
const isoDate = (value: unknown): value is string =>
  typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value);

/** Read the two starter setup existence probes from the native owner. */
export async function nativeStarterProbes(
  actor: Lane3Actor,
  start: string,
  end: string,
): Promise<StarterProbes> {
  if (!object(actor) || !id(actor.holdingId) || !slug(actor.holdingSlug) || !id(actor.companyId))
    throw new ApiError(422, "LANE3_STARTER_PROBES_ACTOR_INVALID", "Use one bounded WorkContext.");
  if (!isoDate(start) || !isoDate(end) || start > end)
    throw new ApiError(422, "LANE3_STARTER_PROBES_INPUT_INVALID", "Use one bounded ISO date range.");
  const config = lane3Config();
  const holding = config.holdings.find(
    (h) => h.holdingId === actor.holdingId && h.holdingSlug === actor.holdingSlug,
  );
  if (!holding)
    throw new ApiError(503, "LANE3_STARTER_PROBES_TARGET_UNAVAILABLE", "The Holding context is not registered.");
  if (holding.companyId !== undefined && holding.companyId !== actor.companyId)
    throw new ApiError(503, "LANE3_STARTER_PROBES_COMPANY_SCOPE_CHANGED", "The Holding company scope changed.");

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/finance/lane3/starter-probes`,
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
          "content-type": "application/json",
          "cache-control": "no-cache",
          "x-erpx-context-holding": holding.holdingId,
          "x-erpx-context-company": actor.companyId,
        },
        body: JSON.stringify({ start, end }),
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
    throw new ApiError(502, "LANE3_STARTER_PROBES_RESPONSE_INVALID", "The native response is invalid.");
  }
  if (!object(payload) || !object(payload.data) || !object(payload.data.probes)) {
    throw new ApiError(502, "LANE3_STARTER_PROBES_RESPONSE_INVALID", "The native response is invalid.");
  }
  const probes = payload.data.probes;
  if (typeof probes.existingChart !== "boolean" || typeof probes.overlappingPeriod !== "boolean") {
    throw new ApiError(502, "LANE3_STARTER_PROBES_RESPONSE_INVALID", "The native response is invalid.");
  }
  return { existingChart: probes.existingChart, overlappingPeriod: probes.overlappingPeriod };
}
