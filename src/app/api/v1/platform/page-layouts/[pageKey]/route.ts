import { pageLayoutRequest } from "@/server/modules/work_projection/public/page-layouts";
import { requireManageErpxContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse } from "@/server/modules/runtime/public/http";
async function handle(request: Request, context: { params: Promise<{ pageKey: string }> }) {
  try {
    await requireManageErpxContext();
    const p=await context.params;
    return pageLayoutRequest(request,p.pageKey,undefined, false);
  } catch(error) { return errorResponse(error); }
}
export { handle as GET, handle as POST };
