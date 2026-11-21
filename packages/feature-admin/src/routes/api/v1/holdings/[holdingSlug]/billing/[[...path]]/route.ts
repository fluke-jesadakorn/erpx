import type { RouteContext } from "@erpx/contracts/next-routes";
import { billingHandler } from "@erpx/server-billing/public/billing-api";
import { nativeBillingInvoiceCommand } from "@erpx/server-billing/public/native-billing-page";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";

export const GET = billingHandler(false);

const retained = billingHandler(false);

/** Native invoice lifecycle (approve/void) dispatches over the committed
 * billing-invoice client. The retained `billingHandler` serves the remaining
 * actions, but its cost-review actions (`cost`, `delete-cost`,
 * `complete-costs`) are native-authoritative: an absent native transport fails
 * closed with a typed 503/409 and never reaches the TypeScript writer. Target,
 * provider, rollout, rating and retry actions stay on the TypeScript command
 * until the owner-side actor policy matches the retained platform-only contract. */
export async function POST(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/billing/[[...path]]">,
) {
  const params = await context.params;
  const segments = params.path ?? [];
  const raw = await request
    .clone()
    .json()
    .catch(() => null);
  const body = raw && typeof raw === "object" && !Array.isArray(raw) ? (raw as Record<string, unknown>) : null;
  const action = typeof body?.action === "string" ? body.action : undefined;
  const create = segments.length === 0 && action === "approve";
  const voidPath =
    segments[0] === "invoices" && typeof segments[1] === "string" && segments[2] === "void";
  const voidBody = segments.length === 0 && action === "void" && typeof body?.invoiceId === "string";
  if (!create && !voidBody && !voidPath) return retained(request, context);
  const key = request.headers.get("Idempotency-Key");
  if (!key)
    return errorResponse(new ApiError(422, "COMMAND_KEY_REQUIRED", "Idempotency-Key is required."));
  try {
    const actor = await requireWorkContext(params.holdingSlug);
    const route = voidPath
      ? `/${segments[1]}/void`
      : voidBody
        ? `/${String(body?.invoiceId)}/void`
        : "";
    const result = await nativeBillingInvoiceCommand(
      {
        userId: actor.userId,
        holdingId: actor.holdingId,
        platform: actor.role === "platform_admin",
        role: actor.role,
        assignmentId: actor.assignmentId,
      },
      actor.holdingId,
      route,
      body,
      key,
    );
    if (result === undefined)
      throw new ApiError(
        503,
        "NATIVE_BILLING_UNAVAILABLE",
        "The native Billing service is unavailable.",
      );
    return Response.json(
      { data: result },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (error) {
    return errorResponse(error);
  }
}
