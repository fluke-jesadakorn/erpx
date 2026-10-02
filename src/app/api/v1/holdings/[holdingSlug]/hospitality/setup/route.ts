import { requireWorkContext } from '@/server/authorization';
import { hospitalitySetup } from '@/server/hospitality-setup';
import { errorResponse } from '@/server/http';
export const dynamic='force-dynamic';
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
  try {
    const actor=await requireWorkContext((await params).holdingSlug),query=new URL(request.url).searchParams;
    return Response.json({data:await hospitalitySetup(actor,query.get('branch')??undefined,query.get('property')??undefined)},{headers:{'Cache-Control':'private, no-store'}});
  } catch(error) {return errorResponse(error);}
}
