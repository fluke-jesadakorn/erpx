import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import {configureCurrencyScale,confirmManualRate,conversionSchema,currencyScaleSchema,manualRateSchema,moneyContext,preferenceSchema,quoteMoneyConversion,saveCurrencyPreference} from '@erpx/server-finance/public/company-money';

export const dynamic = "force-dynamic";
type MoneyRoute = { params: Promise<{ holdingSlug: string; path?: string[] }> };

export async function GET(_request: Request, context: MoneyRoute) {
  try {
    const { holdingSlug, path = [] } = await context.params;
    if (path.length) throw new ApiError(404, "NOT_FOUND", "Money resource not found.");
    return Response.json(canonicalDestinationLinks({ data: await moneyContext(await requireWorkContext(holdingSlug)) }));
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: MoneyRoute) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request)) throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const { holdingSlug, path = [] } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const key = request.headers.get("Idempotency-Key");
    if (!key) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const input = await request.json().catch(() => { throw new ApiError(422, "INVALID_JSON", "Use a JSON body."); });
    if (path.length === 1 && path[0] === "preference") {
      const parsed = preferenceSchema.safeParse(input);
      if (!parsed.success) throw new ApiError(422, "INVALID_CURRENCY", parsed.error.issues[0]?.message ?? "Invalid currency.");
      return Response.json(canonicalDestinationLinks({ data: await saveCurrencyPreference(actor, parsed.data, key) }));
    }
    if (path.length === 1 && path[0] === "convert") {
      const parsed = conversionSchema.safeParse(input);
      if (!parsed.success) throw new ApiError(422, "INVALID_CONVERSION", parsed.error.issues[0]?.message ?? "Invalid conversion request.");
      return Response.json(canonicalDestinationLinks({ data: await quoteMoneyConversion(actor, parsed.data) }));
    }
    if (path.length === 1 && path[0] === "rates") {
      const parsed = manualRateSchema.safeParse(input);
      if (!parsed.success) throw new ApiError(422, "INVALID_FX_RATE", parsed.error.issues[0]?.message ?? "Invalid manual rate.");
      return Response.json(canonicalDestinationLinks({ data: await confirmManualRate(actor, parsed.data, key) }), { status: 201 });
    }
    if (path.length === 1 && path[0] === "scales") {
      const parsed = currencyScaleSchema.safeParse(input);
      if (!parsed.success) throw new ApiError(422, "INVALID_CURRENCY_SCALE", parsed.error.issues[0]?.message ?? "Invalid currency scale.");
      return Response.json(canonicalDestinationLinks({ data: await configureCurrencyScale(actor, parsed.data, key) }));
    }
    throw new ApiError(404, "NOT_FOUND", "Money resource not found.");
  } catch (error) { return errorResponse(error); }
}
