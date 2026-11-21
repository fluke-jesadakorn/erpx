import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { posWorkspace } from "@erpx/server-pos/public/pos";
import { errorResponse } from "@erpx/server-runtime/public/http";
import { nativePosWorkspace, nativePosWorkspaceConfigured } from "@erpx/feature-pos/adapters/server/native-pos-workspace";
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}){
 try{const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),search=new URL(request.url).searchParams;const data=nativePosWorkspaceConfigured()?await nativePosWorkspace(actor,request.headers,search):undefined;return Response.json(canonicalDestinationLinks({data:data??await posWorkspace(actor,search.get('sale')??undefined,search.get('q')??'',search.get('tab')??undefined,search.get('cursor')??undefined,false,search.get('finance')==='1')}),{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
