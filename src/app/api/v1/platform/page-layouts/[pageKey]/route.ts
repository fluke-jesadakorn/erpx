import { pageLayoutRequest } from "@/server/page-layouts";
import { requireManageErpxContext } from "@/server/authorization";
import { errorResponse } from "@/server/http";
async function handle(request: Request, context: { params: Promise<{ pageKey: string }> }) {
  try {
    await requireManageErpxContext();
    const p=await context.params;
    return pageLayoutRequest(request,p.pageKey,undefined, false);
  } catch(error) { return errorResponse(error); }
}
export { handle as GET, handle as POST };
