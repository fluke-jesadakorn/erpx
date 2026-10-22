import {legalPortalUploadRequest} from '@erpx/server-legal/public/portal-upload-request';
import {legalPortalCommandRequest} from '@erpx/server-legal/public/portal-command-request';
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { boundedBody } from "@erpx/server-legal/public/legal/body";
import {
  portalContext,
} from "@erpx/server-legal/public/legal/portal";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import {legalPortalDownloadRequest} from '@erpx/server-legal/public/portal-download-request';
import {legalPortalDetailRequest} from '@erpx/server-legal/public/portal-detail-request';
import {legalPortalListRequest} from '@erpx/server-legal/public/portal-snapshot-request';
export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export async function GET(req: Request, ctx: Context) {
  try {
    const p = await ctx.params,
      path = p.path ?? [];
    if (path.length === 2 && path[1] === "download")
      return await legalPortalDownloadRequest(p.holdingSlug,path[0]);
    if(path.length===1)return Response.json(canonicalDestinationLinks({data:await legalPortalDetailRequest(p.holdingSlug,path[0])}),{headers:{'Cache-Control':'private, no-store'}});
    if(!path.length)return Response.json(canonicalDestinationLinks({data:await legalPortalListRequest(p.holdingSlug)}),{headers:{'Cache-Control':'private, no-store'}});
    await portalContext(p.holdingSlug);
    throw new ApiError(404, "NOT_FOUND", "Unknown portal resource.");
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(req: Request, ctx: Context) {
  try {
    if (req.headers.get("origin") !== requestPublicOrigin(req))
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const key = req.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "A command key is required.",
      );
    const params = await ctx.params;
    await portalContext(params.holdingSlug);
    if (params.path?.length === 2 && params.path[1] === "upload")
      return Response.json(canonicalDestinationLinks({
        data: await legalPortalUploadRequest(
          req,
          params.holdingSlug,
          params.path[0],
          decodeURIComponent(req.headers.get("X-File-Name") ?? "Document"),
          req.headers.get("Content-Type") ?? "",
          await boundedBody(req, 10 * 1024 * 1024),
          key,
        ),
      }));
    if (params.path?.length)
      throw new ApiError(404, "NOT_FOUND", "Unknown portal action.");
    let raw: unknown;
    try {
      raw = JSON.parse(
        new TextDecoder().decode(await boundedBody(req, 500000)),
      );
    } catch (error) {
      if (error instanceof SyntaxError)
        throw new ApiError(422, "INVALID_JSON", "Invalid JSON.");
      throw error;
    }
    return Response.json(
      canonicalDestinationLinks({
        data: await legalPortalCommandRequest(params.holdingSlug, raw, key),
      }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
