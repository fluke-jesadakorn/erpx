import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { rosterCommand, rosterData } from "@/server/modules/people_payroll/public/people-roster";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string }> };
export async function GET(request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const { write } = await holdingResources(actor.holdingId), q = new URL(request.url).searchParams;
    return Response.json(canonicalDestinationLinks({ data: await rosterData(write, actor, { department: q.get("department") ?? undefined, week: q.get("week") ?? undefined }) }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
export async function POST(request: Request, context: Context) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request)) throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin for this action.");
    const actor = await requireWorkContext((await context.params).holdingSlug), { write } = await holdingResources(actor.holdingId);
    const key = request.headers.get("Idempotency-Key");
    if (!key) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    let input: unknown;
    try { input = await request.json(); } catch { throw new ApiError(422, "INVALID_JSON", "Request body must be valid JSON."); }
    return Response.json(canonicalDestinationLinks({ data: await rosterCommand(write as PostgresDatabase, actor, input, key) }));
  } catch (error) { return errorResponse(error); }
}
