import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import {nativeBrandingCommand,nativeBrandingFailure,nativeBrandingWorkspace} from "@erpx/feature-brand/lib/native-branding";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";

const patchSchema = z.object({
  documentMode: z.enum(["holding", "branch", "both"]).optional(),
  remove: z.enum(["primary", "inverse"]).optional(),
  shortName: z.string().trim().min(1).max(40).optional(),
  accentColor: z.string().regex(/^#[0-9a-fA-F]{6}$/).optional(),
  removeLogo: z.boolean().optional(),
}).refine((value) => value.documentMode || value.remove || value.shortName || value.accentColor || value.removeLogo, "Choose a branding change.");
const headers={"Cache-Control":"private, no-store"};
function nativeFailure(error:unknown):never{const failure=nativeBrandingFailure(error);if(failure)throw new ApiError(failure.status,failure.code,failure.message);throw error;}
export async function GET(
  _request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/branding/[[...path]]">,
) {
  try {
    const { holdingSlug } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    let data;
    try { data = await nativeBrandingWorkspace(actor, holdingSlug); } catch (error) { nativeFailure(error); }
    if (!data) throw new ApiError(503, "NATIVE_BRANDING_UNAVAILABLE", "The native Branding reader is unavailable.");
    return Response.json(canonicalDestinationLinks({ data }), { headers });
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
    const commandId = request.headers.get("Idempotency-Key");
    if (!commandId) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Use an Idempotency-Key for branding changes.");
    const actor = await requireWorkContext(holdingSlug);
    const input = patchSchema.parse(await request.json());
    const body = path[0] === "storefront" && path.length === 1
      ? { action: "storefront" as const, shortName: input.shortName, accentColor: input.accentColor, removeLogo: input.removeLogo }
      : path[0] === "holding" && path.length === 1
        ? { action: "profile" as const, scopeKind: "holding" as const, scopeId: actor.holdingId, documentMode: input.documentMode, remove: input.remove }
        : path[0] === "branches" && path[1] && path.length === 2
          ? { action: "profile" as const, scopeKind: "branch" as const, scopeId: path[1], documentMode: input.documentMode, remove: input.remove }
          : null;
    if (!body) throw new ApiError(404, "NOT_FOUND", "Branding target not found.");
    let data;
    try { data = await nativeBrandingCommand(actor, holdingSlug, body, commandId); } catch (error) { nativeFailure(error); }
    if (!data) throw new ApiError(503, "NATIVE_BRANDING_UNAVAILABLE", "The native Branding writer is unavailable.");
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}
