import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { rememberPlatformRole } from "@erpx/server-work_projection/public/workspace-preferences";
import { runtimeEnv } from "../../../../../../../../../packages/server-runtime/src/public/runtime";
import { NextResponse } from "next/server";
import { z } from "zod";
import { requireManageErpxContext, requirePlatformContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";

const schema = z.object({ assignmentId: z.string().min(1).max(128) });

export async function GET() {
  try {
    const actor = await requirePlatformContext();
    return NextResponse.json(canonicalDestinationLinks({ data: { userId: actor.userId, assignmentId: actor.assignmentId } }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request) {
  try {
    const { platform: actor } = await requireManageErpxContext();
    const parsed = schema.safeParse(await request.json());
    if (!parsed.success)
      throw new ApiError(422, "VALIDATION_FAILED", "Choose a valid Platform role.");
    const selected = actor.assignments.find(
      (assignment) => assignment.id === parsed.data.assignmentId,
    );
    if (!selected)
      throw new ApiError(403, "ASSIGNMENT_UNAVAILABLE", "That Platform assignment is unavailable.");
    await rememberPlatformRole((await runtimeEnv()).CONTROL_DB,actor.userId,selected.id);
    const response = NextResponse.json(canonicalDestinationLinks({ data: selected }));
    response.cookies.set("erpx_assignment_platform", selected.id, {
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
