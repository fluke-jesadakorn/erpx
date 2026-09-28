import { z } from "zod";
import { bindRecordAttachmentSet } from "@/server/attachments/service";
import { requireWorkContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
import { holdingResources } from "@/server/runtime";

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
    const compatibilityAdapter = Boolean(
      await write
        .prepare(
          `SELECT 1 FROM attachment_sets set_row
           JOIN attachment_events event ON event.set_id=set_row.id AND event.company_id=set_row.company_id
           WHERE set_row.id=? AND set_row.company_id=? AND event.action='set.created'
             AND COALESCE((event.details_json::jsonb->>'compatibilityAdapter')::boolean,false)=true
           LIMIT 1`,
        )
        .bind(setId, actor.companyId)
        .first(),
    );
    return Response.json({
      data: await bindRecordAttachmentSet(
        write,
        actor,
        setId,
        parsed.data.expectedTargetVersion,
        key,
        { compatibilityAdapter },
      ),
    });
  } catch (error) {
    return errorResponse(error);
  }
}
