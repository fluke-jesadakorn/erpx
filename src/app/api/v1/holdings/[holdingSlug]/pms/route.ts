import { pmsCommand, pmsWorkspace } from "@/server/pms";
import { requireWorkContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";
import { holdingResources } from "@/server/runtime";
import { PostgresDatabase } from "@/server/runtime/postgres";
import { requestPublicOrigin } from "@/server/public-origin";
export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string }> };
export async function GET(request: Request, { params }: Context) {
  try {
    const { holdingSlug } = await params;
    const actor = await requireWorkContext(holdingSlug);
    return Response.json(
      {
        data: await pmsWorkspace(
          actor,
          new URL(request.url).searchParams.get("property") ?? undefined,
          new URL(request.url).searchParams.get("dayControls") === "1",
        ),
      },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, { params }: Context) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the ERPX website to save this change.",
      );
    const { holdingSlug } = await params;
    const actor = await requireWorkContext(holdingSlug),
      key = request.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "IDEMPOTENCY_KEY_REQUIRED",
        "An Idempotency-Key is required.",
      );
    const body = await request.json().catch(() => {
      throw new ApiError(422, "INVALID_JSON", "Invalid data format.");
    });
    const { write } = await holdingResources(actor.holdingId);
    if (!(write instanceof PostgresDatabase))
      throw new Error("POSTGRES_REQUIRED");
    return Response.json({ data: await pmsCommand(write, actor, body, key) });
  } catch (e) {
    return errorResponse(e);
  }
}
