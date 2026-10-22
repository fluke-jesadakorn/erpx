import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { selfServiceSummary } from "@erpx/server-records/public/self-service";
import { errorResponse } from "@erpx/server-runtime/public/http";
export const dynamic="force-dynamic";
export async function GET(_request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try{let count: number | null = null; const data = await selfServiceSummary(await requireWorkContext((await params).holdingSlug), value => { count = value; }); return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store','X-ERPX-Work-Count': count === null ? 'unknown' : String(count)}});}catch(e){return errorResponse(e);}
}
