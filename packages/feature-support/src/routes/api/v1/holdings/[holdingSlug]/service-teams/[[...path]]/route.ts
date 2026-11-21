import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {createNativeSupportServiceTeams} from '../../../../../../../adapters/server/native-support-service-teams';
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
const unavailable=()=>new ApiError(503,'NATIVE_SUPPORT_SERVICE_TEAMS_UNAVAILABLE','The native Support service-team service is unavailable.');
export async function GET(request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params;
    await requireWorkContext(holdingSlug);
    if (path.length)
      throw new ApiError(404, "NOT_FOUND", "Team route not found.");
    const config = process.env.ERPX_NATIVE_SUPPORT_SERVICE_TEAMS_CONFIG;
    if (!config) throw unavailable();
    return Response.json(
      canonicalDestinationLinks({ data: await createNativeSupportServiceTeams(config).options(request.headers, holdingSlug) }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params;
    await requireWorkContext(holdingSlug);
    if (path.length > 1)
      throw new ApiError(404, "NOT_FOUND", "Team route not found.");
    const config = process.env.ERPX_NATIVE_SUPPORT_SERVICE_TEAMS_CONFIG;
    if (!config) throw unavailable();
    const key = request.headers.get("Idempotency-Key");
    if (!key) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "A valid Idempotency-Key is required.");
    return Response.json(canonicalDestinationLinks({
      data: await createNativeSupportServiceTeams(config).save(request.headers, holdingSlug, path[0] ?? null, await request.text(), key),
    }));
  } catch (e) {
    return errorResponse(e);
  }
}
