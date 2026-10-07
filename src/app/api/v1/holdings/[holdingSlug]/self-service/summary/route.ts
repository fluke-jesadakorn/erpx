import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { selfServiceSummary } from "@/server/modules/records/public/self-service";
import { errorResponse } from "@/server/modules/runtime/public/http";
export const dynamic="force-dynamic";
export async function GET(_request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try{let count: number | null = null; const data = await selfServiceSummary(await requireWorkContext((await params).holdingSlug), value => { count = value; }); return Response.json({data},{headers:{'Cache-Control':'private, no-store','X-ERPX-Work-Count': count === null ? 'unknown' : String(count)}});}catch(e){return errorResponse(e);}
}
