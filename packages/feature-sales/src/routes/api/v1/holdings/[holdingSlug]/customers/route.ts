import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {customerDirectory} from "@erpx/server-customer_360/public/customer-timeline";
import {nativeCustomerTimeline} from "@erpx/feature-sales/public/native-customer-timeline-page";
import {errorResponse} from "@erpx/server-runtime/public/http";
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}){try{const a=await requireWorkContext((await params).holdingSlug),q=new URL(request.url).searchParams,id=q.get('id'),native=id?await nativeCustomerTimeline(a,id,q.get('cursor')??undefined,q.get('limit')===null?undefined:Number(q.get('limit'))):undefined;return Response.json(canonicalDestinationLinks({data:native??await customerDirectory(a)}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
