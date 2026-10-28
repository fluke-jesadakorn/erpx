import { nativeReservePost, nativeReserveReadback, nativeIssueReadback, isReserveOrderCommand, isIssueOrderCommand } from "./native-reserve";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {
  changeAgreement,
  changeServiceOrder,
  createAgreement,
  createServiceOrder,
  createServiceRequest,
  fieldServiceDetail,
  fieldServiceWorkspace,
} from "@erpx/server-field_service/public/field-service";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const slug=(await c.params).holdingSlug, query=new URL(r.url).searchParams;
    const issueOrder=query.get("nativeIssueOrder"), issueKey=query.get("nativeIssueKey");
    if(issueOrder!==null || issueKey!==null) {
      if(!issueOrder||!issueKey||[...query.keys()].length!==2) throw new ApiError(404,"NOT_FOUND","Service order not found.");
      const native=await nativeIssueReadback(r,slug,issueOrder,issueKey);
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service order not found.");
    }
    const order=query.get("nativeReserveOrder"), reservation=query.get("nativeReserveId");
    if(order!==null || reservation!==null) {
      if(!order||!reservation||[...query.keys()].length!==2) throw new ApiError(404,"NOT_FOUND","Service order not found.");
      const native=await nativeReserveReadback(r,slug,order,reservation);
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service order not found.");
    }
    const a = await requireWorkContext(slug), id = query.get("id");
    return Response.json(
      canonicalDestinationLinks({
        data: id
          ? await fieldServiceDetail(a, id)
          : await fieldServiceWorkspace(a),
      }),
      { headers },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(r: Request, c: C) {
  try {
    if (
      r.headers.get("origin") &&
      r.headers.get("origin") !== requestPublicOrigin(r)
    )
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const key = r.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const holdingSlug=(await c.params).holdingSlug;
    const a = await requireWorkContext(holdingSlug);
    const original=process.env.ERPX_NATIVE_FIELD_SERVICE_RESERVE_CONFIG === undefined && process.env.ERPX_NATIVE_FIELD_SERVICE_ISSUE_CONFIG === undefined ? undefined : r.clone();
    let input: Awaited<ReturnType<Request["json"]>>;
    try { input=await r.json(); } catch(error) { void original?.body?.cancel().catch(()=>undefined); throw error; }
    if(original && (isReserveOrderCommand(input) && process.env.ERPX_NATIVE_FIELD_SERVICE_RESERVE_CONFIG !== undefined || isIssueOrderCommand(input) && process.env.ERPX_NATIVE_FIELD_SERVICE_ISSUE_CONFIG !== undefined)) {
      const native=await nativeReservePost(original,holdingSlug,input);
      if(native)return native;
      throw new ApiError(503,"NATIVE_SERVICE_UNAVAILABLE","Unable to confirm the reservation.");
    } else { void original?.body?.cancel().catch(()=>undefined); }
    const data =
      input.entity === "request" ? await createServiceRequest(a,input,key) :
      input.entity === "agreement"
        ? typeof input.id === "string"
          ? await changeAgreement(a, input.id, input, key)
          : await createAgreement(a, input, key)
        : typeof input.id === "string"
          ? await changeServiceOrder(a, input.id, input, key)
          : await createServiceOrder(a, input, key);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
