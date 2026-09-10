import { NextResponse } from "next/server";
import { z } from "zod";
import {
  listWorkspaceCompanies,
  requireWorkContext,
} from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";

const schema = z.object({ companyId: z.string().min(1).max(128) });

export async function POST(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/companies/active">,
) {
  try {
    const { holdingSlug } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    if (actor.scope.kind !== "holding")
      throw new ApiError(
        409,
        "COMPANY_DERIVED_FROM_LINEAGE",
        "Local workspaces derive Company from their organization lineage.",
      );
    const parsed = schema.safeParse(await request.json());
    if (!parsed.success)
      throw new ApiError(422, "VALIDATION_FAILED", "Choose a valid Company.");
    const companies = await listWorkspaceCompanies(actor.holdingId);
    if (!companies.some((company) => company.id === parsed.data.companyId))
      throw new ApiError(403, "COMPANY_UNAVAILABLE", "That Company is not available in this Holding.");
    const response = NextResponse.json({ data: { companyId: parsed.data.companyId } });
    response.cookies.set(`erpx_company_${actor.holdingId}`, parsed.data.companyId, {
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
