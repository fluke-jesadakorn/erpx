import { z } from "zod";
import { boundedBody } from "@/server/modules/legal/public/legal/body";
import { portalContext, portalList } from "@/server/modules/legal/public/legal/portal";
import { legalPortalQuickAccess } from "@/server/modules/work_projection/public/portal-quick-access";
import { readQuickAccessPreferences, saveQuickAccessPreferences } from "@/server/modules/work_projection/public/quick-access-preferences";
import { holdingBindingBySlug } from "@/server/modules/runtime/public/runtime";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
const inputSchema = z.object({
  pinnedIds: z.array(z.string().min(1).max(240)).max(6).nullable(),
  expectedVersion: z.number().int().nonnegative(),
}).strict();

export async function GET(_request: Request, context: Context) {
  try {
    const { holdingSlug } = await context.params;
    const binding = await holdingBindingBySlug(holdingSlug);
    const { actor } = await portalContext(holdingSlug);
    const preference = await readQuickAccessPreferences(actor.userId, "legal_portal", binding.id);
    return Response.json({ data: preference }, { headers });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function POST(request: Request, context: Context) {
  try {
    if (request.headers.get("origin") !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const requestKey = request.headers.get("Idempotency-Key");
    if (!requestKey) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "A command key is required.");
    let raw: unknown;
    try {
      raw = JSON.parse(new TextDecoder().decode(await boundedBody(request, 4096)));
    } catch (error) {
      if (error instanceof SyntaxError) throw new ApiError(422, "INVALID_JSON", "Invalid JSON.");
      throw error;
    }
    const { pinnedIds, expectedVersion } = inputSchema.parse(raw);
    const { holdingSlug } = await context.params;
    const binding = await holdingBindingBySlug(holdingSlug);
    const { write, actor } = await portalContext(holdingSlug);
    const availableIds = legalPortalQuickAccess(holdingSlug, await portalList(write, actor)).map((item) => item.id);
    const preference = await saveQuickAccessPreferences({
      userId: actor.userId,
      surface: "legal_portal",
      contextKey: binding.id,
      pinnedIds,
      expectedVersion,
      availableIds,
      requestKey,
    });
    return Response.json({ data: preference }, { headers });
  } catch (error) {
    return errorResponse(error);
  }
}
