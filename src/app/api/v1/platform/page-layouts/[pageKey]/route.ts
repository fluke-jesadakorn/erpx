import { pageLayoutRequest } from "@/server/page-layouts";
async function handle(request: Request, context: { params: Promise<{ pageKey: string }> }) { const p=await context.params; return pageLayoutRequest(request,p.pageKey,undefined, false); }
export { handle as GET, handle as POST };
