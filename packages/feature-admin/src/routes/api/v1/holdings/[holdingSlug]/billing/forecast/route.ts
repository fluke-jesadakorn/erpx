import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {headers} from "next/headers";
import {requestedHoldingScope, requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {createNativeBillingClient,nativeBillingConfiguration,nativeBillingFailure,type NativeBillingPageContext} from "@erpx/server-billing/public/native-billing";
import {ApiError, errorResponse} from "@erpx/server-runtime/public/http";
import {holdingResources} from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import {requireCompanyAuthority} from "@erpx/server-operations/public/staff-policy";

export async function GET(_request: Request, context: { params: Promise<{ holdingSlug: string }> }) {
  try {
    const { holdingSlug } = await context.params;
    const requested = await requestedHoldingScope(holdingSlug);
    if (requested.kind !== "holding")
      throw new ApiError(403, "FORBIDDEN", "The forecast is available in the Holding workspace.");
    const actor = await requireWorkContext(requested);
    if (actor.role !== "holding_owner" || actor.scope.kind !== "holding")
      throw new ApiError(403, "FORBIDDEN", "Only the Holding owner can view this forecast.");
    await requireCompanyAuthority((await holdingResources(actor.holdingId)).write, actor, ["holding_owner"]);
    const raw = process.env.ERPX_NATIVE_BILLING_CONFIG;
    if (raw === undefined)
      throw new ApiError(503, "NATIVE_BILLING_UNAVAILABLE", "The native Billing forecast is not configured.");
    const client = createNativeBillingClient(nativeBillingConfiguration(raw));
    const holding = client.holding(actor.holdingId);
    if (!holding)
      throw new ApiError(503, "NATIVE_BILLING_UNAVAILABLE", "The native Billing Holding target is not configured.");
    const owner: NativeBillingPageContext = {
      userId: actor.userId,
      holdingId: actor.holdingId,
      holdingSlug: holding.holdingSlug,
      platform: false,
      role: actor.role,
      assignmentId: actor.assignmentId,
    };
    const data = await client.forecast(await headers(), owner, { holdingId: owner.holdingId, holdingSlug: owner.holdingSlug });
    return Response.json(canonicalDestinationLinks({ data }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    const mapped = nativeBillingFailure(error);
    return errorResponse(mapped ? new ApiError(mapped.status, mapped.code, mapped.message) : error);
  }
}
