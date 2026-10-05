import { requireWorkContext } from "@/server/authorization";
import { recordSupplementData } from "@/server/record-details";
import { isRecordKind } from "@/lib/record-drawer";
import { ApiError, errorResponse } from "@/server/http";
export const dynamic="force-dynamic";
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try {
  const actor=await requireWorkContext((await params).holdingSlug),query=new URL(request.url).searchParams,kind=query.get('kind')??'',id=query.get('id')??'';
  if(!isRecordKind(kind)||!id||id.length>180)throw new ApiError(422,"RECORD_TARGET_REQUIRED","Choose a valid source record.");
  return Response.json({data:await recordSupplementData(actor,{kind,id},Object.fromEntries(query))},{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){return errorResponse(error);}
}
