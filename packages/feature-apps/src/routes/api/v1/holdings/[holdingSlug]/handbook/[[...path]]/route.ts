import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { serviceCommandBody } from "@erpx/server-support/public/service-http";
import {
  handbookList,
  handbookArticle,
  saveHandbookArticle,
  actOnHandbook,
  setHandbookEditor,
  handbookEditorOptions,
  handbookSchema,
  handbookActionSchema,
  handbookEditorSchema,
} from "@erpx/server-runtime/public/handbook";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export async function GET(request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params,
      actor = await requireWorkContext(holdingSlug),
      { write } = await holdingResources(actor.holdingId),
      query = new URL(request.url).searchParams;
    let data;
    if (path.length === 0)
      data = await handbookList(
        write,
        actor,
        query.get("q") ?? "",
        query.get("edit") === "1",
      );
    else if (path.length === 1)
      data =
        path[0] === "editors"
          ? await handbookEditorOptions(write, actor)
          : await handbookArticle(
              write,
              actor,
              path[0],
              query.get("edit") === "1",
              query.has("revision") ? Number(query.get("revision")) : undefined,
            );
    else throw new ApiError(404, "NOT_FOUND", "Handbook route not found.");
    return Response.json(
      canonicalDestinationLinks({ data }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params,
      actor = await requireWorkContext(holdingSlug),
      { write } = await holdingResources(actor.holdingId),
      { key, input } = await serviceCommandBody(request);
    let data;
    if (!path.length)
      data = await saveHandbookArticle(
        write,
        actor,
        null,
        handbookSchema.parse(input),
        key,
      );
    else if (path.length === 1 && path[0] === "editors")
      data = await setHandbookEditor(
        write,
        actor,
        handbookEditorSchema.parse(input),
        key,
      );
    else if (path.length === 1)
      data = await saveHandbookArticle(
        write,
        actor,
        path[0],
        handbookSchema.parse(input),
        key,
      );
    else if (path.length === 2 && path[1] === "actions")
      data = await actOnHandbook(
        write,
        actor,
        path[0],
        handbookActionSchema.parse(input),
        key,
      );
    else throw new ApiError(404, "NOT_FOUND", "Handbook action not found.");
    return Response.json(canonicalDestinationLinks({ data }));
  } catch (e) {
    return errorResponse(e);
  }
}
