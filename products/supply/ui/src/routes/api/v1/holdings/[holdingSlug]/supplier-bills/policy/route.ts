import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { z } from "zod";

import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
import { requestPublicOrigin } from "@erpx/manage-erpx-server/public/public-origin";
import {
  supplierInvoicePolicySchema,
} from "@erpx/procurement-server/public/supplier-bill-matching";
import {ownedSaveSupplierBillPolicy} from "@erpx/procurement-ui/public/native-supplier-bill-command-owned";

type Context = { params: Promise<{ holdingSlug: string }> };
const nativeUnavailable = () => {
  throw new ApiError(503, "NATIVE_PROCUREMENT_UNAVAILABLE", "The native Procurement service is unavailable.");
};

export async function POST(request: Request, context: Context) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const key = request.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Idempotency-Key is required.");
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const holdingSlug = (await context.params).holdingSlug;
    const input = supplierInvoicePolicySchema.parse(await request.json());
    return Response.json(canonicalDestinationLinks({ data: await ownedSaveSupplierBillPolicy(actor, holdingSlug, input, key) ?? nativeUnavailable() }));
  } catch (error) {
    if (error instanceof z.ZodError)
      return errorResponse(new ApiError(422, "VALIDATION_FAILED", error.issues[0]?.message ?? "Invalid input."));
    if (error instanceof SyntaxError)
      return errorResponse(new ApiError(422, "INVALID_JSON", "Use valid JSON."));
    return errorResponse(error);
  }
}
