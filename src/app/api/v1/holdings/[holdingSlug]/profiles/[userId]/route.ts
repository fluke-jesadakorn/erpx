import { requireWorkContext } from "@/server/authorization";
import { errorResponse } from "@/server/http";
import { memberProfileView } from "@/server/member-profiles";

export async function GET(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/profiles/[userId]">) {
  try {
    const { holdingSlug, userId } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const beforeRaw = new URL(request.url).searchParams.get("before");
    const before = beforeRaw ? Number(beforeRaw) : undefined;
    const beforeId = new URL(request.url).searchParams.get("beforeId") ?? undefined;
    return Response.json({ data: await memberProfileView(actor,userId,Number.isSafeInteger(before) && before! > 0 ? before : undefined,beforeId) },
      { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
