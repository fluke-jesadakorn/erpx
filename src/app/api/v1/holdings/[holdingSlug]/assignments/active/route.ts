import { NextResponse } from "next/server";
import { z } from "zod";
import { requireWorkContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";

const schema = z.object({ assignmentId: z.string().min(1).max(128) });

export async function POST(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/assignments/active">) {
  try {
    const { holdingSlug } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const parsed = schema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Choose a valid work assignment.");
    const selected = actor.assignments.find((item) => item.id === parsed.data.assignmentId);
    if (!selected) throw new ApiError(403, "ASSIGNMENT_UNAVAILABLE", "This assignment is expired, revoked, or belongs to another account.");

    const response = NextResponse.json({ data: selected });
    response.cookies.set(`erpx_assignment_${actor.holdingId}`, selected.id, {
      httpOnly: true,
      sameSite: "lax",
      secure: new URL(request.url).protocol === "https:",
      path: "/",
      maxAge: 60 * 60 * 12,
      priority: "high",
    });
    return response;
  } catch (error) {
    return errorResponse(error);
  }
}
