import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { companyFeatureCatalogue, changeCompanyFeatureRollout } from "@erpx/server-pms/public/hospitality-rollout";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";

type Context = { params: Promise<{holdingSlug:string}> };

export async function GET(_request: Request, {params}: Context) {
  try {
    const actor = await requireWorkContext((await params).holdingSlug);
    return Response.json(canonicalDestinationLinks({data:await companyFeatureCatalogue(actor)}),{headers:{'Cache-Control':'private, no-store'}});
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, {params}: Context) {
  try {
    if (request.headers.get('origin') !== requestPublicOrigin(request))
      throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin');
    const key = request.headers.get('Idempotency-Key');
    if (!key) throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required');
    const actor = await requireWorkContext((await params).holdingSlug);
    const result = await changeCompanyFeatureRollout(actor,await request.json(),key);
    return Response.json(canonicalDestinationLinks({data:result}),{headers:{'Cache-Control':'private, no-store'}});
  } catch (error) { return errorResponse(error); }
}
