import { z } from "zod";
import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { iamActorFromWork } from "@/server/iam-context";
import { requestPublicOrigin } from "@/server/public-origin";
import { ApiError, errorResponse } from "@/server/http";
import type { PostgresDatabase } from "@/server/runtime/postgres";
import {
  technologyData,
  previewProvisioning,
  applyProvisioning,
  retryProvisioning,
} from "@/server/technology";
import { provisioningInput } from "@/lib/technology";
import { directoryOperationsApi } from "@/server/directory-operations-api";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export const dynamic = "force-dynamic";
export async function GET(_request: Request, context: Context) {
  try {
    const { holdingSlug } = await context.params,
      actor = await requireWorkContext(holdingSlug),
      { write } = await holdingResources(actor.holdingId);
    return Response.json(
      {
        data: await technologyData(
          write as PostgresDatabase,
          iamActorFromWork(actor),
        ),
      },
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
    const key = request.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "IDEMPOTENCY_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const { holdingSlug, path = [] } = await context.params,
      contextActor = await requireWorkContext(holdingSlug),
      actor = iamActorFromWork(contextActor),
      { write } = await holdingResources(contextActor.holdingId),
      db = write as PostgresDatabase,
      raw = await request.json();
    if (path[0] === "directory")
      return Response.json({
        data: await directoryOperationsApi(
          db,
          contextActor,
          path.slice(1),
          raw,
          key,
        ),
      });
    if (path[0] === "provisioning" && path[1] === "preview")
      return Response.json({
        data: await previewProvisioning(
          db,
          actor,
          provisioningInput.parse(raw),
        ),
      });
    if (path[0] === "provisioning" && path[1] === "retry")
      return Response.json({
        data: await retryProvisioning(
          db,
          actor,
          z.string().parse(raw.id),
          z.number().int().parse(raw.expectedVersion),
          key,
        ),
      });
    if (path[0] === "provisioning" && path[1] === "apply")
      return Response.json({
        data: await applyProvisioning(
          db,
          actor,
          provisioningInput.parse(raw),
          z.string().parse(raw.previewToken),
          key,
        ),
      });
    throw new ApiError(404, "NOT_FOUND", "Technology operation not found.");
  } catch (e) {
    return errorResponse(e);
  }
}
