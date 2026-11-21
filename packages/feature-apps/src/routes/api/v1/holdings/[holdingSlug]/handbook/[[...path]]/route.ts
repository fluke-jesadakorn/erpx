import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {createNativeHandbookClient} from "@erpx/feature-collaboration/lib/native-handbook";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { serviceCommandBody } from "@erpx/server-support/public/service-http";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
const headers = {"Cache-Control":"private, no-store"};
function client() {
  const config = process.env.ERPX_NATIVE_HANDBOOK_CONFIG;
  if (!config)
    throw new ApiError(
      503,
      "NATIVE_HANDBOOK_UNAVAILABLE",
      "The native handbook service is unavailable.",
    );
  return createNativeHandbookClient(config);
}
export async function GET(request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params;
    await requireWorkContext(holdingSlug);
    const query = new URL(request.url).searchParams;
    const native = client();
    let data: unknown;
    if (path.length === 0)
      data = await native.list(request.headers, holdingSlug, {
        q: query.get("q") ?? undefined,
        edit: query.get("edit") === "1",
        cursor: query.get("cursor") ?? undefined,
        limit: query.has("limit") ? Number(query.get("limit")) : undefined,
      });
    else if (path.length === 1 && path[0] === "editors")
      data = await native.editors(request.headers, holdingSlug);
    else if (path.length === 1)
      data = await native.article(request.headers, holdingSlug, path[0], {
        edit: query.get("edit") === "1",
        revision: query.has("revision") ? Number(query.get("revision")) : undefined,
      });
    else throw new ApiError(404, "NOT_FOUND", "Handbook route not found.");
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params;
    await requireWorkContext(holdingSlug);
    const { key, input } = await serviceCommandBody(request);
    const body = JSON.stringify(input);
    const native = client();
    let target: string;
    if (path.length === 0) target = "";
    else if (path.length === 1 && path[0] === "editors") target = "/editors";
    else if (path.length === 1) target = `/${encodeURIComponent(path[0])}`;
    else if (path.length === 2 && path[1] === "actions")
      target = `/${encodeURIComponent(path[0])}/actions`;
    else throw new ApiError(404, "NOT_FOUND", "Handbook action not found.");
    const data = await native.command(
      request.headers,
      holdingSlug,
      target,
      body,
      key,
    );
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
