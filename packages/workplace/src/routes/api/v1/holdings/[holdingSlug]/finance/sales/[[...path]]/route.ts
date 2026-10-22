import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { salesWorkspace, salesDefaultsWorkspace, prepareSalesDefaults, decideSalesDefaults, createSalesInvoiceDraft, issueCustomerReceipt, customerReceiptDetail, customerReceiptExport } from "@erpx/server-finance/public/sales";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params;
    const actor = requirePermission(await requireWorkContext(holdingSlug), "finance.read");
    if (!path.length) return Response.json(canonicalDestinationLinks({ data: await salesWorkspace(actor, Object.fromEntries(new URL(request.url).searchParams)) }), { headers });
    if (path[0] === "defaults" && path.length === 1) return Response.json(canonicalDestinationLinks({ data: await salesDefaultsWorkspace(actor) }), { headers });
    if (path[0] === "receipts" && path[1] && path.length === 2) return Response.json(canonicalDestinationLinks({ data: await customerReceiptDetail(actor, path[1]) }), { headers });
    if (path[0] === "receipts" && path[1] && path[2] === "export" && path.length === 3) {
      const result = await customerReceiptExport(actor, path[1]);
      return new Response(result.content, { headers: { ...headers, "Content-Type": "text/html; charset=utf-8", "Content-Security-Policy": "default-src 'none'; style-src 'unsafe-inline'", "X-Content-Type-Options": "nosniff", "X-Document-SHA256": result.sha256 } });
    }
    throw new ApiError(404, "NOT_FOUND", "Sales interface not found.");
  } catch (error) { return failure(error); }
}
export async function POST(request: Request, context: Context) {
  try {
    const origin = request.headers.get("origin"); if (origin && origin !== requestPublicOrigin(request)) throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const { holdingSlug, path = [] } = await context.params;
    const actor = requirePermission(await requireWorkContext(holdingSlug), "finance.read");
    const key = request.headers.get("Idempotency-Key"); if (!key || key.length > 200) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "A valid Idempotency-Key is required.");
    const input = await request.json(); let data;
    if (path[0] === "defaults" && path.length === 1) data = await prepareSalesDefaults(actor, input, key);
    else if (path[0] === "defaults" && path.length === 3 && path[2] === "decide") data = await decideSalesDefaults(actor, path[1], input, key);
    else if (path[0] === "invoices" && path.length === 1) data = await createSalesInvoiceDraft(actor, input, key);
    else if (path[0] === "receipts" && path.length === 3 && path[2] === "issue") data = await issueCustomerReceipt(actor, path[1], key);
    else throw new ApiError(404, "NOT_FOUND", "Sales command not found.");
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) { return failure(error); }
}
function failure(error: unknown) {
  if (error instanceof z.ZodError) return errorResponse(new ApiError(422, "VALIDATION_FAILED", error.issues[0]?.message ?? "Check the sales form."));
  if (error instanceof SyntaxError) return errorResponse(new ApiError(422, "INVALID_JSON", "Use valid JSON."));
  return errorResponse(error);
}
