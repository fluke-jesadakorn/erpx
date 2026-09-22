import { z } from "zod";

import { requireWorkContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
import {
  saveSupplierInvoicePolicy,
  supplierInvoicePolicy,
  supplierInvoicePolicySchema,
} from "@/server/supplier-bill-matching";

type Context = { params: Promise<{ holdingSlug: string }> };

export async function GET(_request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json(
      { data: await supplierInvoicePolicy(actor) },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (error) {
    return errorResponse(error);
  }
}

export async function POST(request: Request, context: Context) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const key = request.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Idempotency-Key is required.");
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const input = supplierInvoicePolicySchema.parse(await request.json());
    return Response.json({ data: await saveSupplierInvoicePolicy(actor, input, key) });
  } catch (error) {
    if (error instanceof z.ZodError)
      return errorResponse(new ApiError(422, "VALIDATION_FAILED", error.issues[0]?.message ?? "Invalid input."));
    if (error instanceof SyntaxError)
      return errorResponse(new ApiError(422, "INVALID_JSON", "Use valid JSON."));
    return errorResponse(error);
  }
}
