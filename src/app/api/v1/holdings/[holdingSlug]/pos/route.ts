import { requireWorkContext } from '@/server/authorization';
import { posWorkspace } from '@/server/pos';
import { errorResponse } from '@/server/http';
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}){
 try{const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug);return Response.json({data:await posWorkspace(actor,new URL(request.url).searchParams.get('sale')??undefined,new URL(request.url).searchParams.get('q')??'',new URL(request.url).searchParams.get('tab')??undefined,new URL(request.url).searchParams.get('cursor')??undefined)},{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
