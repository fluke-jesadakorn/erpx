import { z } from "zod";
import { frontAppForHost } from "@/lib/shared/front-host";
import { currentSession } from "@/server/modules/identity_access/public/authorization";
import { frontCompanyChoices, frontCompanyItems } from "@/server/modules/work_projection/public/front-quick-access";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { readQuickAccessPreferences, saveQuickAccessPreferences } from "@/server/modules/work_projection/public/quick-access-preferences";

export const dynamic = "force-dynamic";

function frontApp(request: Request) {
  const app = frontAppForHost(request.headers.get("host"));
  if (!app) throw new ApiError(404, "NOT_FOUND", "This Front picker is unavailable.");
  return app;
}

export async function GET(request: Request) {
  try {
    const app = frontApp(request);
    const { session } = await currentSession();
    await frontCompanyChoices(app);
    return Response.json({ data: await readQuickAccessPreferences(session.user.id, "front-picker", app) }, { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function POST(request: Request) {
  try {
    const app = frontApp(request);
    if (request.headers.get("origin") !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const { session } = await currentSession();
    const requestKey = request.headers.get("Idempotency-Key");
    if (!requestKey) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "An Idempotency-Key is required.");
    const input = z.object({
      pinnedIds: z.array(z.string().min(1).max(200)).max(6).nullable(),
      expectedVersion: z.number().int().nonnegative(),
    }).parse(await request.json());
    const availableIds = frontCompanyItems(await frontCompanyChoices(app)).map((item) => item.id);
    return Response.json({ data: await saveQuickAccessPreferences({
      userId: session.user.id,
      surface: "front-picker",
      contextKey: app,
      pinnedIds: input.pinnedIds,
      expectedVersion: input.expectedVersion,
      availableIds,
      requestKey,
    }) }, { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}
