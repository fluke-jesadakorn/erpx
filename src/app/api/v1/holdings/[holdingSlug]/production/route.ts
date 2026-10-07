import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import {
  changeManufacturingOrder,
  createPlanningScenario,
  cancelPlanningScenario,
  createSpecification,
  productionDetail,
  productionWorkspace,
  reviewPlanningRecommendation,
  reviewSpecification,
} from "@/server/modules/manufacturing/public/production";
import { createManufacturingAccountingProfile, reviewManufacturingAccountingProfile } from "@/server/modules/manufacturing/public/production-accounting";
import { saveProductionResource, saveProductionResourceCalendar } from "@/server/modules/manufacturing/public/production-resources";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const a = await requireWorkContext((await c.params).holdingSlug),
      u = new URL(r.url),
      id = u.searchParams.get("id"),
      workspaceModule =
        u.searchParams.get("module") === "planning"
          ? "planning"
          : "manufacturing";
    return Response.json(
      {
        data: id
          ? await productionDetail(a, id)
          : await productionWorkspace(a, workspaceModule),
      },
      { headers },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(r: Request, c: C) {
  try {
    if (
      r.headers.get("origin") &&
      r.headers.get("origin") !== requestPublicOrigin(r)
    )
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const key = r.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const a = await requireWorkContext((await c.params).holdingSlug),
      i = await r.json();
    let data;
    if (i.entity === "accounting")
      data = typeof i.id === "string" ? await reviewManufacturingAccountingProfile(a, i.id, i, key) : await createManufacturingAccountingProfile(a, i, key);
    else if (i.entity === "resource")
      data = await saveProductionResource(a, typeof i.id === "string" ? i.id : null, i, key);
    else if (i.entity === "resource-calendar")
      data = await saveProductionResourceCalendar(a, i.id, i, key);
    else if (i.entity === "specification")
      data =
        typeof i.id === "string"
          ? await reviewSpecification(a, i.id, i, key)
          : await createSpecification(a, i, key);
    else if (i.entity === "scenario")
      data = typeof i.id === "string" ? await cancelPlanningScenario(a, i.id, i, key) : await createPlanningScenario(a, i, key);
    else if (i.entity === "recommendation")
      data = await reviewPlanningRecommendation(a, i.id, i, key);
    else data = await changeManufacturingOrder(a, i.id, i, key);
    return Response.json({ data }, { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
