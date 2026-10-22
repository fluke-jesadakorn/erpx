import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {holdingResources} from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import {errorResponse} from "@erpx/server-runtime/public/http";
import {pmsRangeReport,pmsReportEntries} from "@erpx/server-pms/public/pms/reports";
type Context={params:Promise<{holdingSlug:string}>};
export async function GET(request:Request,{params}:Context){
 try{
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId),q=new URL(request.url).searchParams;
  const data=q.get('view')==='entries'?await pmsReportEntries(write,actor,q.get('property')??'',q.get('date')??'',Number(q.get('page')??1)):await pmsRangeReport(write,actor,{propertyId:q.get('property'),from:q.get('from'),to:q.get('to'),asOf:q.get('asOf')??undefined,compareAsOf:q.get('compareAsOf')??undefined});
  return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){return errorResponse(error);}
}
