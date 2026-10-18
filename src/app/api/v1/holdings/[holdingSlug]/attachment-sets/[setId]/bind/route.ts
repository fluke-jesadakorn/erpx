import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import {appAttachmentSources} from "@/server/modules/files/public/app-attachment-sources";
import { z } from "zod";
import { bindRecordAttachmentSet } from "@/server/modules/files/public/attachments/service";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { holdingResources } from "@/server/modules/runtime/public/runtime";

const schema = z.object({ expectedTargetVersion: z.number().int().positive() });

export async function POST(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-sets/[setId]/bind">,
) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const key = request.headers.get("Idempotency-Key");
    if (!key || !/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "A valid Idempotency-Key is required.",
      );
    const { holdingSlug, setId } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const parsed = schema.safeParse(await request.json());
    if (!parsed.success)
      throw new ApiError(
        422,
        "VALIDATION_FAILED",
        parsed.error.issues[0]?.message ?? "Invalid record document bind.",
      );
    const { write } = await holdingResources(actor.holdingId);
    const compatibilityAdapter = await appAttachmentSources(write).compatibilitySet(setId,actor.companyId);
    return Response.json(canonicalDestinationLinks({
      data: await bindRecordAttachmentSet(
        write,
        actor,
        setId,
        parsed.data.expectedTargetVersion,
        key,
        { compatibilityAdapter },
      ),
    }));
  } catch (error) {
    return errorResponse(error);
  }
}
