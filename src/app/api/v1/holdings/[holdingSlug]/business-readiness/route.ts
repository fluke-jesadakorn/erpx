import {requireWorkContext} from '@/server/authorization';
import {businessReadiness} from '@/server/business-readiness';
import {errorResponse} from '@/server/http';
export async function GET(_request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try{return Response.json({data:await businessReadiness(await requireWorkContext((await params).holdingSlug))},{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
