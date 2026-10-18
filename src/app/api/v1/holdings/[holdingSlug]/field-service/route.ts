import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import {
  changeAgreement,
  changeServiceOrder,
  createAgreement,
  createServiceOrder,
  createServiceRequest,
  fieldServiceDetail,
  fieldServiceWorkspace,
} from "@/server/modules/field_service/public/field-service";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const a = await requireWorkContext((await c.params).holdingSlug),
      id = new URL(r.url).searchParams.get("id");
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
    const a = await requireWorkContext((await c.params).holdingSlug),
      input = await r.json();
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
