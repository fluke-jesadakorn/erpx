import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { pmsCommand, pmsWorkspace } from "../../../../../../../../../packages/server-pms/src/public/pms";
import { nativePmsAvailable, nativePmsFailure, nativePmsRoomBoard } from "../../../../../../lib/native-pms";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { holdingResources } from "../../../../../../../../../packages/server-runtime/src/public/runtime";
import { isTransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { pmsReadSelectionFromSearch } from "@erpx/server-pms/public/rules/pms-read-selection";
import { measureReadRequest } from "@erpx/server-runtime/public/runtime/read-request";
import { measureReadPhase } from "@erpx/server-runtime/public/runtime/read-metrics";
import { productHostForHost } from "@erpx/feature-runtime/lib/product-hosts";
import { pmsProductOperationAllowed } from "@erpx/feature-runtime/lib/product-api-policy";
export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string }> };
export async function GET(request: Request, { params }: Context) {
  return measureReadRequest("pms", async () => {
  try {
    const { holdingSlug } = await params;
    const actor = await requireWorkContext(holdingSlug);
    const search = new URL(request.url).searchParams;
    const section = search.get("section");
    if (search.get("view") === "list" && (section === "rooms" || section === "housekeeping")) {
      if (!nativePmsAvailable())
        throw new ApiError(503, "NATIVE_PMS_UNAVAILABLE", "The native Hotel board is unavailable.");
      const value = (key: string) => search.get(key) ?? undefined;
      let data;
      try {
        data = await nativePmsRoomBoard(actor, holdingSlug, section, {
          property: value("property"),
          status: value("status"),
          q: value("q"),
          building: value("building"),
          floor: value("floor"),
          typeId: value("typeId"),
          page: value("page"),
          selectedId: value("selectedId") ?? value("record") ?? value("detailId"),
          from: value("from"),
          to: value("to"),
        });
      } catch (error) {
        const failure = nativePmsFailure(error);
        throw failure ? new ApiError(failure.status, failure.code, failure.message) : error;
      }
      if (!data)
        throw new ApiError(503, "NATIVE_PMS_UNAVAILABLE", "The native Hotel board is unavailable.");
      return Response.json(canonicalDestinationLinks({ data }), {
        headers: { "Cache-Control": "private, no-store" },
      });
    }
    return Response.json(
      canonicalDestinationLinks({
        data: await pmsWorkspace(
          actor,
          search.get("property") ?? undefined,
          search.get("dayControls") === "1",
          false,
          pmsReadSelectionFromSearch(search),
        ),
      }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
  });
}
export async function POST(request: Request, { params }: Context) {
  return measureReadRequest("pms-command", async () => {
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
    if (!pmsProductOperationAllowed(productHostForHost(request.headers.get("host")), body?.operation))
      throw new ApiError(404, "NOT_FOUND", "This Hotel action is unavailable on this product.");
    const { write } = await holdingResources(actor.holdingId);
    if (!isTransactionalSqlDatabase(write))
      throw new Error("POSTGRES_REQUIRED");
    return Response.json(canonicalDestinationLinks({ data: await measureReadPhase("pms_command", () => pmsCommand(write, actor, body, key)) }));
  } catch (e) {
    return errorResponse(e);
  }
  });
}
