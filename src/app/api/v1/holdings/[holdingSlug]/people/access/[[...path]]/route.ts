import { iamContext } from "@/server/iam-context";
import { readManagement } from "@/server/user-management";
import {
  accessComparison,
  accessDetail,
  accessEntries,
  accessMetadata,
  peopleAccessData,
} from "@/server/people-access";
import { ApiError, errorResponse } from "@/server/http";
export const dynamic = "force-dynamic";
export async function GET(
  request: Request,
  context: { params: Promise<{ holdingSlug: string; path?: string[] }> },
) {
  try {
    const { holdingSlug, path = [] } = await context.params,
      { db, actor } = await iamContext(holdingSlug);
    if (path.join("/") === "management") return Response.json({data:await readManagement(db,actor)},{headers:{"Cache-Control":"private, no-store"}});
    const data = await peopleAccessData(db, actor),
      params = new URL(request.url).searchParams;
    let result: unknown;
    if (!path.length || path.join("/") === "tree")
      result = accessMetadata(data);
    else if (path.join("/") === "entries") result = accessEntries(data, params);
    else if (path.length === 2 && path[0] === "objects")
      result = accessDetail(data, path[1]);
    else if (path.join("/") === "compare")
      result = accessComparison(data, params.getAll("assignment"));
    else
      throw new ApiError(404, "NOT_FOUND", "People access resource not found.");
    return Response.json(
      { data: result },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (error) {
    return errorResponse(error);
  }
}
