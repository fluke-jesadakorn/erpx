import {appAttachmentSources} from "@/server/modules/files/public/app-attachment-sources";
import { z } from "zod";
import { documentBrandModes, listBrandProfiles, updateBrandProfile, updateStorefrontAppBrand } from "@/server/modules/brand/public/branding";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";

const patchSchema = z.object({
  documentMode: z.enum(documentBrandModes).optional(),
  remove: z.enum(["primary", "inverse"]).optional(),
  shortName: z.string().trim().min(1).max(40).optional(),
  accentColor: z.string().regex(/^#[0-9a-fA-F]{6}$/).optional(),
  removeLogo: z.boolean().optional(),
}).refine((value) => value.documentMode || value.remove || value.shortName || value.accentColor || value.removeLogo, "Choose a branding change.");

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
    const { write, files } = await holdingResources(actor.holdingId);
    if (path[0] === "storefront" && path.length === 1) {
      const changed = await updateStorefrontAppBrand(write, actor, {
        shortName: input.shortName,
        accentColor: input.accentColor,
        removeLogo: input.removeLogo,
        commandId,
      });
      if (changed.previousId) {
        const old = await appAttachmentSources(write).previousObject(changed.previousId);
        if (old?.object_key) await files.delete(old.object_key).catch(() => undefined);
      }
      return Response.json({ data: changed.result });
    }
    const target = path[0] === "holding" && path.length === 1
      ? { scopeKind: "holding" as const, scopeId: actor.holdingId }
      : path[0] === "branches" && path[1] && path.length === 2
        ? { scopeKind: "branch" as const, scopeId: path[1] }
        : null;
    if (!target) throw new ApiError(404, "NOT_FOUND", "Brand profile not found.");
    const changed = await updateBrandProfile(write, actor, { ...target, ...input, commandId });
    if (changed.previousId) {
      const old = await appAttachmentSources(write).previousObject(changed.previousId);
      if (old?.object_key) await files.delete(old.object_key).catch(() => undefined);
    }
    return Response.json({ data: changed.result });
  } catch (error) {
    return errorResponse(error);
  }
}
