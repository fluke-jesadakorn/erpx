import { canonicalDestinationLinks } from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { ownedNativeQualityCancel } from "@erpx/feature-quality/public/native-quality";

type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };

export async function POST(request: Request, context: Context) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const { holdingSlug, path = [] } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const key = request.headers.get("Idempotency-Key");
    if (!key) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Idempotency-Key is required.");
    const input = await request.json();
    if (path.length !== 2 || path[1] !== "cancel")
      throw new ApiError(404, "NOT_FOUND", "Quality route not found.");
    const data = await ownedNativeQualityCancel(actor, holdingSlug, path[0]!, input, key);
    if (!data) throw new ApiError(503, "NATIVE_QUALITY_UNAVAILABLE", "The native Quality service is unavailable.");
    return Response.json(canonicalDestinationLinks({ data }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}
