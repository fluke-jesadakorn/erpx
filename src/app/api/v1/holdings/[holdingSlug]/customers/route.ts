import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {customerDirectory,customerTimeline} from "@/server/modules/customer_360/public/customer-timeline";
import {errorResponse} from "@/server/modules/runtime/public/http";
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}){try{const a=await requireWorkContext((await params).holdingSlug),q=new URL(request.url).searchParams,id=q.get('id');return Response.json(canonicalDestinationLinks({data:id?await customerTimeline(a,id,{cursor:q.get('cursor')??undefined,limit:q.get('limit')??undefined}):await customerDirectory(a)}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
