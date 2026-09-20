import {requireWorkContext} from '@/server/authorization';
import {routingAttention} from '@/server/routing-attention';
import {errorResponse} from '@/server/http';
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try {const actor=await requireWorkContext((await params).holdingSlug);return Response.json({data:await routingAttention(actor,Object.fromEntries(new URL(request.url).searchParams))},{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
