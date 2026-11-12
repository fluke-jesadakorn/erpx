import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {
  changeManufacturingOrder,
  createPlanningScenario,
  cancelPlanningScenario,
  productionDetail,
  productionWorkspace,
  reviewPlanningRecommendation,
} from "@erpx/server-manufacturing/public/production";
import { createManufacturingAccountingProfile, reviewManufacturingAccountingProfile } from "@erpx/server-manufacturing/public/production-accounting";
import { saveProductionResource, saveProductionResourceCalendar } from "@erpx/server-manufacturing/public/production-resources";
import { nativePlanningCommand } from "@erpx/feature-manufacturing/public/native-planning-workspace-page";
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
      canonicalDestinationLinks({
        data: id
          ? await productionDetail(a, id)
          : await productionWorkspace(a, workspaceModule),
      }),
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
    const nativePlanning =
      i.entity === "resource" ||
      i.entity === "resource-calendar" ||
      i.entity === "scenario" ||
      i.entity === "recommendation";
    if (nativePlanning) {
      const path =
        i.entity === "resource"
          ? typeof i.id === "string"
            ? `/planning/resources/${encodeURIComponent(i.id)}`
            : "/planning/resources"
          : i.entity === "resource-calendar"
            ? `/planning/resources/${encodeURIComponent(i.id)}/calendar`
            : i.entity === "scenario"
              ? typeof i.id === "string"
                ? `/planning/scenarios/${encodeURIComponent(i.id)}`
                : "/planning/scenarios"
              : `/planning/recommendations/${encodeURIComponent(i.id)}`;
      const result = await nativePlanningCommand(
        r.headers,
        (await import("@erpx/feature-manufacturing/public/native-planning-workspace-page")).nativePlanningPageContext(a),
        a.holdingSlug,
        path,
        i,
        key,
      );
      if (result === undefined)
        throw new ApiError(503, "NATIVE_PLANNING_UNAVAILABLE", "The native Planning service is unavailable.");
      return Response.json(canonicalDestinationLinks({ data: result }), { headers });
    }
    const nativeSpecification = i.entity === "specification";
    if (nativeSpecification) {
      const path =
        typeof i.id === "string"
          ? `/manufacturing/specifications/${encodeURIComponent(i.id)}`
          : "/manufacturing/specifications";
      const result = await nativePlanningCommand(
        r.headers,
        (await import("@erpx/feature-manufacturing/public/native-planning-workspace-page")).nativePlanningPageContext(a),
        a.holdingSlug,
        path,
        i,
        key,
      );
      if (result === undefined)
        throw new ApiError(503, "NATIVE_MANUFACTURING_UNAVAILABLE", "The native Manufacturing service is unavailable.");
      return Response.json(canonicalDestinationLinks({ data: result }), { headers });
    }
    if (i.entity === "accounting")
      data = typeof i.id === "string" ? await reviewManufacturingAccountingProfile(a, i.id, i, key) : await createManufacturingAccountingProfile(a, i, key);
    else if (i.entity === "resource")
      data = await saveProductionResource(a, typeof i.id === "string" ? i.id : null, i, key);
    else if (i.entity === "resource-calendar")
      data = await saveProductionResourceCalendar(a, i.id, i, key);
    else if (i.entity === "scenario")
      data = typeof i.id === "string" ? await cancelPlanningScenario(a, i.id, i, key) : await createPlanningScenario(a, i, key);
    else if (i.entity === "recommendation")
      data = await reviewPlanningRecommendation(a, i.id, i, key);
    else data = await changeManufacturingOrder(a, i.id, i, key);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
