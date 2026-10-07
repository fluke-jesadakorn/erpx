import { boundedBody } from "@/server/modules/legal/public/legal/body";
import {
  portalContext,
  portalUpload,
  portalList,
  portalDetail,
  portalDownload,
  portalCommand,
} from "@/server/modules/legal/public/legal/portal";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export async function GET(req: Request, ctx: Context) {
  try {
    const p = await ctx.params,
      { write, files, actor } = await portalContext(p.holdingSlug),
      path = p.path ?? [];
    if (path.length === 2 && path[1] === "download")
      return await portalDownload(write, files, actor, path[0]);
    if (path.length > 1)
      throw new ApiError(404, "NOT_FOUND", "Unknown portal resource.");
    return Response.json(
      {
        data: path.length
          ? await portalDetail(write, actor, path[0])
          : await portalList(write, actor),
      },
      { headers: { "Cache-Control": "private, no-store" } },
    );
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
    const { write, files, actor } = await portalContext(params.holdingSlug);
    if (params.path?.length === 2 && params.path[1] === "upload")
      return Response.json({
        data: await portalUpload(
          write as PostgresDatabase,
          files,
          actor,
          params.path[0],
          decodeURIComponent(req.headers.get("X-File-Name") ?? "Document"),
          req.headers.get("Content-Type") ?? "",
          await boundedBody(req, 10 * 1024 * 1024),
          key,
        ),
      });
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
      {
        data: await portalCommand(write as PostgresDatabase, actor, raw, key),
      },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
