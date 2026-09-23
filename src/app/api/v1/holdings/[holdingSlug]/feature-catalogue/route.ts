import { requireWorkContext } from '@/server/authorization';
import { companyFeatureCatalogue, changeCompanyFeatureRollout } from '@/server/hospitality-rollout';
import { ApiError, errorResponse } from '@/server/http';
import { requestPublicOrigin } from '@/server/public-origin';

type Context = { params: Promise<{holdingSlug:string}> };

export async function GET(_request: Request, {params}: Context) {
  try {
    const actor = await requireWorkContext((await params).holdingSlug);
    return Response.json({data:await companyFeatureCatalogue(actor)},{headers:{'Cache-Control':'private, no-store'}});
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
    return Response.json({data:result},{headers:{'Cache-Control':'private, no-store'}});
  } catch (error) { return errorResponse(error); }
}
