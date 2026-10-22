import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { pmsCommand, pmsWorkspace } from "../../../../../../../../../packages/server-pms/src/public/pms";
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
    return Response.json(
      canonicalDestinationLinks({
        data: await pmsWorkspace(
          actor,
          new URL(request.url).searchParams.get("property") ?? undefined,
          new URL(request.url).searchParams.get("dayControls") === "1",
          false,
          pmsReadSelectionFromSearch(new URL(request.url).searchParams),
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
