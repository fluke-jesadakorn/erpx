import { z } from "zod";
import { documentBrandModes, listBrandProfiles, updateBrandProfile } from "@/server/branding";
import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { ApiError, errorResponse } from "@/server/http";

const patchSchema = z.object({
  documentMode: z.enum(documentBrandModes).optional(),
  remove: z.enum(["primary", "inverse"]).optional(),
}).refine((value) => value.documentMode || value.remove, "Choose a branding change.");

export async function GET(
  _request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/branding/[[...path]]">,
) {
  try {
    const { holdingSlug } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const { write } = await holdingResources(actor.holdingId);
    return Response.json({ data: await listBrandProfiles(write, actor) });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function PATCH(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/branding/[[...path]]">,
) {
  try {
    const { holdingSlug, path = [] } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const input = patchSchema.parse(await request.json());
    const commandId = request.headers.get("Idempotency-Key");
    if (!commandId) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Use an Idempotency-Key for branding changes.");
    const target = path[0] === "holding" && path.length === 1
      ? { scopeKind: "holding" as const, scopeId: actor.holdingId }
      : path[0] === "branches" && path[1] && path.length === 2
        ? { scopeKind: "branch" as const, scopeId: path[1] }
        : null;
    if (!target) throw new ApiError(404, "NOT_FOUND", "Brand profile not found.");
    const { write, files } = await holdingResources(actor.holdingId);
    const changed = await updateBrandProfile(write, actor, { ...target, ...input, commandId });
    if (changed.previousId) {
      const old = await write.prepare("SELECT object_key FROM attachments WHERE id=?").bind(changed.previousId).first<{ object_key: string }>();
      if (old?.object_key) await files.delete(old.object_key).catch(() => undefined);
    }
    return Response.json({ data: changed.result });
  } catch (error) {
    return errorResponse(error);
  }
}
