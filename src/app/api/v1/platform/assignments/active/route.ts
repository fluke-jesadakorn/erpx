import { rememberPlatformRole } from "@/server/workspace-preferences";
import { runtimeEnv } from "@/server/runtime";
import { NextResponse } from "next/server";
import { z } from "zod";
import { requirePlatformContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";

const schema = z.object({ assignmentId: z.string().min(1).max(128) });

export async function POST(request: Request) {
  try {
    const actor = await requirePlatformContext();
    const parsed = schema.safeParse(await request.json());
    if (!parsed.success)
      throw new ApiError(422, "VALIDATION_FAILED", "Choose a valid Platform role.");
    const selected = actor.assignments.find(
      (assignment) => assignment.id === parsed.data.assignmentId,
    );
    if (!selected)
      throw new ApiError(403, "ASSIGNMENT_UNAVAILABLE", "That Platform assignment is unavailable.");
    await rememberPlatformRole((await runtimeEnv()).CONTROL_DB,actor.userId,selected.id);
    const response = NextResponse.json({ data: selected });
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
