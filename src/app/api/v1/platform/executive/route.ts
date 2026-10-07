import { requireManageErpxContext } from "@/server/modules/identity_access/public/authorization";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { platformExecutiveSnapshot, platformExecutiveSources, requirePlatformExecutive } from "@/server/modules/platform_administration/public/platform-executive";

export const dynamic = "force-dynamic";
const headers = { "Cache-Control": "private, no-store" };

export async function GET(request: Request) {
  try {
    const before = requirePlatformExecutive((await requireManageErpxContext()).platform);
    const env = await runtimeEnv();
    const query = new URL(request.url).searchParams;
    const filters = query.has("from") || query.has("to")
      ? { from: query.get("from"), to: query.get("to") }
      : undefined;
    const data = query.has('metric')?await platformExecutiveSources(before,env.CONTROL_DB,env.CONTROL_READ_DB,{metric:query.get('metric'),label:query.get('group'),from:query.get('from'),to:query.get('to'),page:query.get('page')??'1'}):await platformExecutiveSnapshot(before, env.CONTROL_DB, env.CONTROL_READ_DB, filters);
    const after = requirePlatformExecutive((await requireManageErpxContext()).platform);
    if (before.assignmentId !== after.assignmentId
      || before.iam?.roleRevision !== after.iam?.roleRevision
      || before.iam?.identityVersion !== after.iam?.identityVersion)
      throw new ApiError(403, "PLATFORM_AUTHORITY_CHANGED", "Your Platform access changed during this read. Refresh and try again.");
    return Response.json({ data }, { headers });
  } catch (error) {
    return errorResponse(error);
  }
}
