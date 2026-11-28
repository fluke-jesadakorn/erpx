import {requireWorkContext} from '@erpx/identity-access-server/public/authorization';
import {posStartupPrerequisites} from '@erpx/finance-server/public/pos-startup-prerequisites';
import {posPostingSetup} from '@erpx/commerce-server/public/pos-posting-setup';
import {posStartupWorkspace} from '@erpx/pos-server/public/startup';
import {errorResponse} from '@erpx/platform-runtime-server/public/http';
export async function GET(_request:Request,{params}:{params:Promise<{holdingSlug:string}>}){
 try{const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[prerequisites,mapping,workspace]=await Promise.all([posStartupPrerequisites(actor),posPostingSetup(actor),posStartupWorkspace(actor)]);return Response.json({data:{prerequisites,mapping,workspace}},{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
