import { rememberWorkspace } from "@/server/modules/work_projection/public/workspace-preferences";
import { holdingBindingBySlug, runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { NextResponse } from "next/server";
import { z } from "zod";
import {
  listWorkspaceCompanies,
  requestedHoldingScope,
  requireAccountAccess,
} from "@/server/modules/identity_access/public/authorization";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";

const schema = z.object({ companyId: z.string().min(1).max(128) });

export async function POST(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/companies/active">,
) {
  try {
    const { holdingSlug } = await context.params;
    const requested = await requestedHoldingScope(holdingSlug);
    if (requested.kind !== "holding") {
      // Local Company is fixed by lineage. Authenticate against that lineage,
      // including revocation checks, before rejecting Company selection.
      await requireAccountAccess(requested);
      throw new ApiError(
        409,
        "COMPANY_DERIVED_FROM_LINEAGE",
        "Local workspaces derive Company from their organization lineage.",
      );
    }
    // This endpoint is how an account with no remembered Company chooses one.
    // Resolving a scope first raises COMPANY_REQUIRED for exactly those
    // accounts, so authorize against the requested Company instead and keep the
    // eligibility decision below, before any workspace is written.
    const parsed = schema.safeParse(await request.json());
    if (!parsed.success)
      throw new ApiError(422, "VALIDATION_FAILED", "Choose a valid Company.");
    // holdingBindingBySlug raises HOLDING_NOT_FOUND, which errorResponse maps to
    // the same 404 the previous scope-first ordering produced.
    const companies = await listWorkspaceCompanies((await holdingBindingBySlug(holdingSlug)).id);
    if (!companies.some((company) => company.id === parsed.data.companyId))
      throw new ApiError(403, "COMPANY_UNAVAILABLE", "That Company is not available in this Holding.");
    const actor = await requireAccountAccess(requested, { companyId: parsed.data.companyId });
    if (actor.scope.kind !== "holding")
      throw new ApiError(
        409,
        "COMPANY_DERIVED_FROM_LINEAGE",
        "Local workspaces derive Company from their organization lineage.",
      );
    if (actor.scope.companyId !== parsed.data.companyId)
      throw new ApiError(403, "COMPANY_UNAVAILABLE", "That Company is not available in this Holding.");
    await rememberWorkspace((await runtimeEnv()).CONTROL_DB,actor.userId,`/holdings/${holdingSlug}/home`,{holdingId:actor.scope.holdingId,companyId:parsed.data.companyId});
    const response = NextResponse.json({ data: { companyId: parsed.data.companyId } });
    response.cookies.set(`erpx_company_${actor.scope.holdingId}`, parsed.data.companyId, {
      httpOnly: true,
      sameSite: "lax",
      secure: new URL(request.url).protocol === "https:",
      path: "/",
      maxAge: 60 * 60 * 24 * 365,
      priority: "high",
    });
    return response;
  } catch (error) {
    return errorResponse(error);
  }
}
