import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { selfServiceSummary } from "@erpx/records-server/public/self-service";
import { errorResponse } from "@erpx/platform-runtime-server/public/http";
export const dynamic="force-dynamic";
export async function GET(_request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try{let count: number | null = null; const data = await selfServiceSummary(await requireWorkContext((await params).holdingSlug), value => { count = value; }); return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store','X-ERPX-Work-Count': count === null ? 'unknown' : String(count)}});}catch(e){return errorResponse(e);}
}
