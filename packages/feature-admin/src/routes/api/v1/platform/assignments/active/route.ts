import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { randomUUID } from "node:crypto";
import { runtimeEnv } from "../../../../../../../../../packages/server-runtime/src/public/runtime";
import { isTransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { NextResponse } from "next/server";
import { z } from "zod";
import { requireManageErpxContext, requirePlatformContext } from "@erpx/server-identity_access/public/authorization";
import { sessionFromRequest } from "@erpx/server-identity_access/public/account-session";
import { providerContextStore } from "@erpx/server-identity_access/adapters/provider-context-store";
import { ssoAudienceForHost } from "@erpx/feature-runtime/lib/product-hosts";
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
    if (!actor.assignments.some((assignment) => assignment.id === parsed.data.assignmentId))
      throw new ApiError(403, "ASSIGNMENT_UNAVAILABLE", "That Platform assignment is unavailable.");
    const origin = request.headers.get("origin");
    if (origin && origin !== new URL(request.url).origin)
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Choose a Platform role from the current Administration application.");
    const session = await sessionFromRequest(request);
    if (!session) throw new ApiError(401, "SIGN_IN_REQUIRED", "Sign in to Administration.");
    const owner = (await runtimeEnv()).CONTROL_DB;
    if (!isTransactionalSqlDatabase(owner))
      throw new ApiError(503, "PROVIDER_CONTEXT_TRANSACTION_REQUIRED", "Platform role selection is unavailable.");
    const store = providerContextStore(owner, {
      userId: session.user.id,
      sessionId: session.session.id,
      selectedCookie: null,
      audience: ssoAudienceForHost(request.headers.get("host")) ?? undefined,
      remember: true,
    });
    const seed = await store.read();
    const choice = seed.choices.find((item) => item.id === parsed.data.assignmentId);
    if (!choice) throw new ApiError(403, "ASSIGNMENT_UNAVAILABLE", "That Platform assignment is unavailable.");
    const selected = await store.select({
      accountKey: seed.accountKey,
      expectedContext: seed.contextKey,
      expectedRevision: seed.revision,
      assignmentId: choice.id,
      choiceVersion: choice.version,
    }, randomUUID());
    const response = NextResponse.json(canonicalDestinationLinks({ data: { id: choice.id, role: choice.label } }));
    response.cookies.set("erpx_assignment_platform", selected.selectedId, {
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
