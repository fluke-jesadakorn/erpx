import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { staffBookingSearch, bookingError } from "@erpx/server-pms/public/pms-booking";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
export const dynamic = "force-dynamic";
export async function POST(
  request: Request,
  { params }: { params: Promise<{ holdingSlug: string; action: string }> },
) {
  try {
    if (
      request.headers.get("origin") &&
      request.headers.get("origin") !== requestPublicOrigin(request)
    )
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the ERPX website");
    const { holdingSlug, action } = await params;
    if (action !== "availability" && action !== "quote")
      throw new ApiError(404, "NOT_FOUND", "Not found");
    const actor = await requireWorkContext(holdingSlug),
      v = z
        .object({ propertyId: z.string().min(1), input: z.unknown() })
        .parse(await request.json());
    return Response.json(
      canonicalDestinationLinks({ data: await staffBookingSearch(actor, v.propertyId, action, v.input) }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(bookingError(e));
  }
}
