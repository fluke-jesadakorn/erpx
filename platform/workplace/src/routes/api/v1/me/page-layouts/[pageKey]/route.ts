import { pageLayoutRequest } from "@erpx/work-projection-server/public/page-layouts";
async function handle(request: Request, context: { params: Promise<{ pageKey: string }> }) { const p=await context.params; return pageLayoutRequest(request,p.pageKey,undefined, true); }
export { handle as GET, handle as POST };
