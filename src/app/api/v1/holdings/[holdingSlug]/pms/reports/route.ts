import {requireWorkContext} from '@/server/authorization';
import {holdingResources} from '@/server/runtime';
import {errorResponse} from '@/server/http';
import {pmsRangeReport,pmsReportEntries} from '@/server/pms/reports';
type Context={params:Promise<{holdingSlug:string}>};
export async function GET(request:Request,{params}:Context){
 try{
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId),q=new URL(request.url).searchParams;
  const data=q.get('view')==='entries'?await pmsReportEntries(write,actor,q.get('property')??'',q.get('date')??'',Number(q.get('page')??1)):await pmsRangeReport(write,actor,{propertyId:q.get('property'),from:q.get('from'),to:q.get('to')});
  return Response.json({data},{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){return errorResponse(error);}
}
