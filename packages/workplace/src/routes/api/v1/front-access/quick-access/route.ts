import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { frontAppForHost } from "@erpx/feature-shared/lib/front-host";
import {frontQuickAccessRequest} from '@erpx/server-work_projection/public/front-quick-access-request';
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";

export const dynamic = "force-dynamic";

function frontApp(request: Request) {
  const app = frontAppForHost(request.headers.get("host"));
  if (!app) throw new ApiError(404, "NOT_FOUND", "This Front picker is unavailable.");
  return app;
}

export async function GET(request: Request) {
  try {
    const app = frontApp(request);
    return Response.json(canonicalDestinationLinks({ data: await frontQuickAccessRequest(request,app) }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function POST(request: Request) {
  try {
    const app = frontApp(request);
    if (request.headers.get("origin") !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const preference=await frontQuickAccessRequest(request,app,async()=>{
      const requestKey = request.headers.get("Idempotency-Key");
      if (!requestKey) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "An Idempotency-Key is required.");
      const input = z.object({
        pinnedIds: z.array(z.string().min(1).max(200)).max(6).nullable(),
        expectedVersion: z.number().int().nonnegative(),
      }).parse(await request.json());
      return {...input,requestKey};
    });
    return Response.json(canonicalDestinationLinks({ data: preference }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}
