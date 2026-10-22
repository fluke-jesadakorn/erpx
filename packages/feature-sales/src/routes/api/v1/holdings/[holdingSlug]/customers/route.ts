import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {customerDirectory,customerTimeline} from "@erpx/server-customer_360/public/customer-timeline";
import {errorResponse} from "@erpx/server-runtime/public/http";
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}){try{const a=await requireWorkContext((await params).holdingSlug),q=new URL(request.url).searchParams,id=q.get('id');return Response.json(canonicalDestinationLinks({data:id?await customerTimeline(a,id,{cursor:q.get('cursor')??undefined,limit:q.get('limit')??undefined}):await customerDirectory(a)}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
