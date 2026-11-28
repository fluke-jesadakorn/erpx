import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { recordSupplementData } from "@erpx/records-server/public/record-details";
import { isRecordKind } from "@erpx/records-ui/lib/record-drawer";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
export const dynamic="force-dynamic";
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try {
  const actor=await requireWorkContext((await params).holdingSlug),query=new URL(request.url).searchParams,kind=query.get('kind')??'',id=query.get('id')??'';
  if(!isRecordKind(kind)||!id||id.length>180)throw new ApiError(422,"RECORD_TARGET_REQUIRED","Choose a valid source record.");
  return Response.json(canonicalDestinationLinks({data:await recordSupplementData(actor,{kind,id},Object.fromEntries(query))}),{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){return errorResponse(error);}
}
