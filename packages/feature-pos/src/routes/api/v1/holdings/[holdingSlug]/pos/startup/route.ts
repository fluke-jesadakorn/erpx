import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {posStartupPrerequisites} from '@erpx/server-finance/public/pos-startup-prerequisites';
import {posPostingSetup} from '@erpx/server-commerce/public/pos-posting-setup';
import {posStartupWorkspace} from '@erpx/server-pos/public/startup';
import {errorResponse} from '@erpx/server-runtime/public/http';
export async function GET(_request:Request,{params}:{params:Promise<{holdingSlug:string}>}){
 try{const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[prerequisites,mapping,workspace]=await Promise.all([posStartupPrerequisites(actor),posPostingSetup(actor),posStartupWorkspace(actor)]);return Response.json({data:{prerequisites,mapping,workspace}},{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
