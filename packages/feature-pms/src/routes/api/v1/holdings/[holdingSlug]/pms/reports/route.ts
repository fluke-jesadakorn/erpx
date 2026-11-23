import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {ApiError, errorResponse} from "@erpx/server-runtime/public/http";
import {nativePmsFailure, type NativePmsReportData, type NativePmsReportEntriesData} from "../../../../../../../adapters/server/native-pms";
import {nativePmsReport} from "../../../../../../../adapters/server/native-pms-command";
type Context={params:Promise<{holdingSlug:string}>};
const noStore={'Cache-Control':'private, no-store'};
export async function GET(request:Request,{params}:Context){
 try{
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),q=new URL(request.url).searchParams;
  let data:NativePmsReportData|NativePmsReportEntriesData|undefined;
  let native:
   | Awaited<ReturnType<typeof nativePmsReport>>
   | undefined;
  try{
   if(q.get('view')==='entries'){
    native=await nativePmsReport(request,actor,holdingSlug,{view:'entries',property:q.get('property')??'',date:q.get('date')??'',page:Number(q.get('page')??1)});
   }else{
    const asOf=q.get('asOf')??undefined,compareAsOf=q.get('compareAsOf')??undefined;
    native=await nativePmsReport(request,actor,holdingSlug,{view:'range',property:q.get('property')??'',from:q.get('from')??'',to:q.get('to')??'',...(asOf?{asOf}:{}),...(compareAsOf?{compareAsOf}:{})});
   }
  }catch(error){
   const failure=nativePmsFailure(error);
   if(failure) throw new ApiError(failure.status,failure.code,failure.message);
   throw error;
  }
  if(!native) throw new ApiError(503,'NATIVE_PMS_UNAVAILABLE','The native Hotel report is unavailable.');
  if(q.get('view')==='entries'){
   const entries=native as NativePmsReportEntriesData;
   data={rows:entries.rows.map(row=>({id:row.id,...row.data})),hasNext:entries.hasNext,page:entries.page};
  }else{
   data=native as NativePmsReportData;
  }
  return Response.json(canonicalDestinationLinks({data}),{headers:noStore});
 }catch(error){return errorResponse(error);}
}
