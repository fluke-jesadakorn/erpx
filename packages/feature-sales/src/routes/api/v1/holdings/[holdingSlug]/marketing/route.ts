import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {
  changeMarketing,
  createMarketingAsset,
  createMarketingCandidate,
  marketingDetail,
  marketingWorkspace,
} from "@erpx/server-marketing/public/marketing";
import {changeMarketingCampaign} from "@erpx/server-marketing/public/marketing-campaigns";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const a = await requireWorkContext((await c.params).holdingSlug),
      id = new URL(r.url).searchParams.get("id");
    return Response.json(
      canonicalDestinationLinks({ data: id ? await marketingDetail(a, id) : await marketingWorkspace(a) }),
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
      i = await r.json(),
      data =
        i.entity === 'campaign'
          ? await changeMarketingCampaign(a,i,key)
          : typeof i.id === "string"
          ? await changeMarketing(a, i.entity, i.id, i, key)
          : i.entity === "asset"
            ? await createMarketingAsset(a, i, key)
            : await createMarketingCandidate(a, i, key);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
