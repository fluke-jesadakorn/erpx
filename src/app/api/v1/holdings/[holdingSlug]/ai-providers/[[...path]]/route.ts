import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { z } from "zod";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { runtimeEnv, holdingResources } from "@/server/modules/runtime/public/runtime";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import {
  activateAiConnection,
  aiConnectionSchema,
  aiProviderOverview,
  aiSelectionSchema,
  aiVersionSchema,
  disableAiConnection,
  discoverAiModels,
  discoverAiModelsInput,
  aiDiscoverySchema,
  saveAiConnection,
  selectAiProviders,
  testAiConnection,
} from "@/server/modules/agent/public/ai-providers";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export async function GET(_request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params,
      actor = await requireWorkContext(holdingSlug),
      { write } = await holdingResources(actor.holdingId),
      env = await runtimeEnv();
    const data =
      path.length === 0
        ? await aiProviderOverview(write, actor, env)
        : path.length === 2 && path[1] === "models"
          ? await discoverAiModels(write, actor, env, path[0])
          : null;
    if (!data)
      throw new ApiError(404, "NOT_FOUND", "AI settings resource not found.");
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
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const { holdingSlug, path = [] } = await context.params,
      actor = await requireWorkContext(holdingSlug),
      { write } = await holdingResources(actor.holdingId),
      env = await runtimeEnv(),
      key = request.headers.get("Idempotency-Key");
    if (path.length === 1 && path[0] === "discover") {
      const data = await discoverAiModelsInput(write, actor, env, aiDiscoverySchema.parse(await request.json()));
      return Response.json(canonicalDestinationLinks({ data }), { headers: { "Cache-Control": "private, no-store" } });
    }
    if (!key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const input = await request.json();
    let data: unknown;
    if (path.length === 0)
      data = await saveAiConnection(
        write,
        actor,
        env,
        null,
        aiConnectionSchema.parse(input),
        key,
      );
    else if (path.length === 1 && path[0] === "selection")
      data = await selectAiProviders(
        write,
        actor,
        aiSelectionSchema.parse(input),
        key,
      );
    else if (path.length === 1)
      data = await saveAiConnection(
        write,
        actor,
        env,
        path[0],
        aiConnectionSchema.parse(input),
        key,
      );
    else if (path.length === 2 && path[1] === "test")
      data = await testAiConnection(
        write,
        actor,
        env,
        path[0],
        aiVersionSchema.parse(input).expectedVersion,
        key,
      );
    else if (path.length === 2 && path[1] === "activate")
      data = await activateAiConnection(
        write,
        actor,
        path[0],
        aiVersionSchema.parse(input).expectedVersion,
        key,
      );
    else if (path.length === 2 && ["disable", "delete"].includes(path[1]))
      data = await disableAiConnection(
        write,
        actor,
        path[0],
        {
          ...aiVersionSchema
            .extend({ disableUses: z.boolean().default(false) })
            .parse(input),
          remove: path[1] === "delete",
        },
        key,
      );
    else throw new ApiError(404, "NOT_FOUND", "AI settings action not found.");
    return Response.json(canonicalDestinationLinks({ data }), { status: path.length === 0 ? 201 : 200 });
  } catch (e) {
    if (e instanceof z.ZodError)
      return errorResponse(
        new ApiError(
          422,
          "VALIDATION_FAILED",
          e.issues[0]?.message ?? "Invalid settings.",
        ),
      );
    if (e instanceof SyntaxError)
      return errorResponse(
        new ApiError(422, "INVALID_JSON", "Use valid JSON."),
      );
    return errorResponse(e);
  }
}
