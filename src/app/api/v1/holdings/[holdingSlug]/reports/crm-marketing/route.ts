import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {errorResponse} from "@/server/modules/runtime/public/http";
import {crmAnalytics,marketingAnalytics,defaultAnalyticsRange} from "@/server/modules/crm/public/crm-marketing-report";
type Context={params:Promise<{holdingSlug:string}>};
export async function GET(request:Request,context:Context){
 try{
  const a=await requireWorkContext((await context.params).holdingSlug),q=new URL(request.url).searchParams;
  const defaults=defaultAnalyticsRange(),raw={from:q.get('from')??defaults.from,to:q.get('to')??defaults.to,branchId:q.get('branchId')||undefined,propertyId:q.get('propertyId')||undefined,campaignId:q.get('campaignId')||undefined,ownerId:q.get('ownerId')||undefined};
  const data=q.get('kind')==='marketing'?await marketingAnalytics(a,raw):await crmAnalytics(a,raw);
  return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){return errorResponse(error);}
}
