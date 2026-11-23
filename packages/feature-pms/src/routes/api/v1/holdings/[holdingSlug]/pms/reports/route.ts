import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {holdingResources} from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import {ApiError, errorResponse} from "@erpx/server-runtime/public/http";
import {pmsRangeReport,pmsReportEntries} from "@erpx/server-pms/public/pms/reports";
import {nativePmsFailure, type NativePmsReportEntriesData} from "../../../../../../../adapters/server/native-pms";
import {nativePmsReport} from "../../../../../../../adapters/server/native-pms-command";
type Context={params:Promise<{holdingSlug:string}>};
const noStore={'Cache-Control':'private, no-store'};
export async function GET(request:Request,{params}:Context){
 try{
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId),q=new URL(request.url).searchParams;
  if(q.get('view')==='entries'){
   const property=q.get('property')??'',date=q.get('date')??'',page=Number(q.get('page')??1);
   let data;
   try{
    const native=await nativePmsReport(request,actor,holdingSlug,{view:'entries',property,date,page}) as NativePmsReportEntriesData|undefined;
    if(native) data={rows:native.rows.map(row=>({id:row.id,...row.data})),hasNext:native.hasNext,page:native.page};
   }catch(error){
    const failure=nativePmsFailure(error);
    if(failure) throw new ApiError(failure.status,failure.code,failure.message);
    throw error;
   }
   if(!data) data=await pmsReportEntries(write,actor,property,date,page);
   return Response.json(canonicalDestinationLinks({data}),{headers:noStore});
  }
  // The native projection covers PMS-owned day/flow/night facts only. The range
  // view still needs Finance journal balances and booking pace/channel windows
  // from their owner ports, so it stays TypeScript until those ports land.
  const data=await pmsRangeReport(write,actor,{propertyId:q.get('property'),from:q.get('from'),to:q.get('to'),asOf:q.get('asOf')??undefined,compareAsOf:q.get('compareAsOf')??undefined});
  return Response.json(canonicalDestinationLinks({data}),{headers:noStore});
 }catch(error){return errorResponse(error);}
}
