import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { changeManufacturingOrder } from "@erpx/server-manufacturing/public/production";
import { nativeAccountingProfileCommand, nativeManufacturingWave3Available } from "@erpx/feature-manufacturing/adapters/server/native-manufacturing-wave3";
import { nativePlanningCommand } from "@erpx/feature-manufacturing/public/native-planning-workspace-page";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
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
    if (i.action === "issue" || i.action === "pass-qc" || i.action === "cancel" || i.action === "scrap") {
      const result = await nativePlanningCommand(
        r.headers,
        (await import("@erpx/feature-manufacturing/public/native-planning-workspace-page")).nativePlanningPageContext(a),
        a.holdingSlug,
        `/manufacturing/orders/${encodeURIComponent(i.id)}/${encodeURIComponent(i.action)}`,
        i,
        key,
      );
      if (result === undefined)
        throw new ApiError(
          503,
          "NATIVE_MANUFACTURING_UNAVAILABLE",
          "The native Manufacturing order command is unavailable.",
        );
      return Response.json(canonicalDestinationLinks({ data: result }), { headers });
    }
    if (i.entity === "accounting") {
      // Wave 3: the axum accounting-profile route is the only writer. No TS SQL fallback.
      if (!nativeManufacturingWave3Available())
        throw new ApiError(503, "NATIVE_MANUFACTURING_UNAVAILABLE", "The native Manufacturing accounting command is unavailable.");
      const result = await nativeAccountingProfileCommand(
        r.headers,
        a,
        (await c.params).holdingSlug,
        typeof i.id === "string" ? { review: i.id } : { create: true },
        i,
        key,
      );
      if (!result)
        throw new ApiError(503, "NATIVE_MANUFACTURING_UNAVAILABLE", "The native Manufacturing accounting command is unavailable.");
      return Response.json(canonicalDestinationLinks({ data: result }), { headers });
    }
    else data = await changeManufacturingOrder(a, i.id, i, key);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
