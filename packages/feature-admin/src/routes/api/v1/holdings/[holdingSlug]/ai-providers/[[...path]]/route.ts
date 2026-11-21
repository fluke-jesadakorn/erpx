import { canonicalDestinationLinks } from "@erpx/feature-work_projection/lib/destination-links";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {
  createNativeAgentAiProviders,
  nativeAgentAiProvidersAvailable,
} from "@erpx/feature-platform_administration/adapters/server/native-agent-ai-providers";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
const headers = { "Cache-Control": "private, no-store" };
function nativeClient() {
  const raw = process.env.ERPX_NATIVE_AGENT_AI_PROVIDERS_CONFIG;
  if (!raw || !nativeAgentAiProvidersAvailable())
    throw new ApiError(
      503,
      "NATIVE_AGENT_AI_PROVIDERS_UNAVAILABLE",
      "The native AI provider service is unavailable.",
    );
  return createNativeAgentAiProviders(raw);
}
export async function GET(request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params;
    await requireWorkContext(holdingSlug);
    const client = nativeClient();
    const data =
      path.length === 0
        ? await client.overview(request.headers, holdingSlug)
        : path.length === 2 && path[1] === "models"
          ? await client.models(request.headers, holdingSlug, path[0])
          : null;
    if (!data)
      throw new ApiError(404, "NOT_FOUND", "AI settings resource not found.");
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, context: Context) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const { holdingSlug, path = [] } = await context.params;
    await requireWorkContext(holdingSlug);
    const discover = path.length === 1 && path[0] === "discover";
    const key = request.headers.get("Idempotency-Key");
    if (!discover && !key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    let input: unknown;
    try {
      input = await request.json();
    } catch {
      throw new ApiError(422, "INVALID_JSON", "Use valid JSON.");
    }
    if (!input || typeof input !== "object" || Array.isArray(input))
      throw new ApiError(422, "AI_REQUEST_INVALID", "Use one bounded AI provider request.");
    const data = await nativeClient().command(
      request.headers,
      holdingSlug,
      path.length ? `/${path.map(encodeURIComponent).join("/")}` : "",
      JSON.stringify(input),
      discover ? undefined : (key ?? undefined),
    );
    return Response.json(canonicalDestinationLinks({ data }), {
      status: path.length === 0 ? 201 : 200,
    });
  } catch (e) {
    return errorResponse(e);
  }
}
