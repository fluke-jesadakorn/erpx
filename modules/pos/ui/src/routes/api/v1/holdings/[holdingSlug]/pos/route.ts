import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { posWorkspace } from "@erpx/pos-server/public/pos";
import { errorResponse } from "@erpx/platform-runtime-server/public/http";
import { nativePosWorkspace, nativePosWorkspaceEligible } from "@erpx/pos-ui/adapters/server/native-pos-workspace";
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}){
 try{const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),search=new URL(request.url).searchParams;const data=nativePosWorkspaceEligible(search)?await nativePosWorkspace(actor,request.headers,search):undefined;return Response.json(canonicalDestinationLinks({data:data??await posWorkspace(actor,search.get('sale')??undefined,search.get('q')??'',search.get('tab')??undefined,search.get('cursor')??undefined,false,search.get('finance')==='1')}),{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
