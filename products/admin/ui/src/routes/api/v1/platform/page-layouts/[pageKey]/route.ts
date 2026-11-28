import { pageLayoutRequest } from "@erpx/work-projection-server/public/page-layouts";
import { requireManageErpxContext } from "@erpx/identity-access-server/public/authorization";
import { errorResponse } from "@erpx/platform-runtime-server/public/http";
async function handle(request: Request, context: { params: Promise<{ pageKey: string }> }) {
  try {
    await requireManageErpxContext();
    const p=await context.params;
    return pageLayoutRequest(request,p.pageKey,undefined, false);
  } catch(error) { return errorResponse(error); }
}
export { handle as GET, handle as POST };
