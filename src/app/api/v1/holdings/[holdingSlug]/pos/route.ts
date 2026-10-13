import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { posWorkspace } from "@/server/modules/pos/public/pos";
import { errorResponse } from "@/server/modules/runtime/public/http";
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}){
 try{const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug);return Response.json({data:await posWorkspace(actor,new URL(request.url).searchParams.get('sale')??undefined,new URL(request.url).searchParams.get('q')??'',new URL(request.url).searchParams.get('tab')??undefined,new URL(request.url).searchParams.get('cursor')??undefined,false,new URL(request.url).searchParams.get('finance')==='1')},{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
