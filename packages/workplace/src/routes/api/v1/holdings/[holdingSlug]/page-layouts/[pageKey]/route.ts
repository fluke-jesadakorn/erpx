import { pageLayoutRequest } from "@erpx/server-work_projection/public/page-layouts";
async function handle(request: Request, context: { params: Promise<{ pageKey: string; holdingSlug: string }> }) { const p=await context.params; return pageLayoutRequest(request,p.pageKey,p.holdingSlug); }
export { handle as GET, handle as POST };
