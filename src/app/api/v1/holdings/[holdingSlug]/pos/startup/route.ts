import {requireWorkContext} from '@/server/modules/identity_access/public/authorization';
import {posStartupPrerequisites} from '@/server/modules/finance/public/pos-startup-prerequisites';
import {posPostingSetup} from '@/server/modules/commerce/public/pos-posting-setup';
import {posStartupWorkspace} from '@/server/modules/pos/public/startup';
import {errorResponse} from '@/server/modules/runtime/public/http';
export async function GET(_request:Request,{params}:{params:Promise<{holdingSlug:string}>}){
 try{const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[prerequisites,mapping,workspace]=await Promise.all([posStartupPrerequisites(actor),posPostingSetup(actor),posStartupWorkspace(actor)]);return Response.json({data:{prerequisites,mapping,workspace}},{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
