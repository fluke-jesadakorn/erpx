import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { previewExpense } from "@/server/modules/people_payroll/expense";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";

export async function POST(request: Request, context: { params: Promise<{ holdingSlug: string }> }) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json({ data: await previewExpense(actor, await request.json()) }, { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}
