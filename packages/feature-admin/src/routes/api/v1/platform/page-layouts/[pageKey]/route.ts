import { pageLayoutRequest } from "@erpx/server-work_projection/public/page-layouts";
import { requireManageErpxContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse } from "@erpx/server-runtime/public/http";
async function handle(request: Request, context: { params: Promise<{ pageKey: string }> }) {
  try {
    await requireManageErpxContext();
    const p=await context.params;
    return pageLayoutRequest(request,p.pageKey,undefined, false);
  } catch(error) { return errorResponse(error); }
}
export { handle as GET, handle as POST };
