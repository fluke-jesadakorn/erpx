import { requireWorkContext } from "@/server/authorization";
import { requestPublicOrigin } from "@/server/public-origin";
import { ApiError, errorResponse } from "@/server/http";
import {
  changeAgreement,
  changeServiceOrder,
  createAgreement,
  createServiceOrder,
  fieldServiceDetail,
  fieldServiceWorkspace,
} from "@/server/field-service";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const a = await requireWorkContext((await c.params).holdingSlug),
      id = new URL(r.url).searchParams.get("id");
    return Response.json(
      {
        data: id
          ? await fieldServiceDetail(a, id)
          : await fieldServiceWorkspace(a),
      },
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
      input.entity === "agreement"
        ? typeof input.id === "string"
          ? await changeAgreement(a, input.id, input, key)
          : await createAgreement(a, input, key)
        : typeof input.id === "string"
          ? await changeServiceOrder(a, input.id, input, key)
          : await createServiceOrder(a, input, key);
    return Response.json({ data }, { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
