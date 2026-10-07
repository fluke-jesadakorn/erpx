import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {requestPublicOrigin} from "@/server/modules/manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@/server/modules/runtime/public/http";
import {projectGraph,projectHandoffs,projectStructureCommand} from "@/server/modules/projects/public/project-structure";
type Context={params:Promise<{holdingSlug:string;id:string}>};
const headers={'Cache-Control':'private, no-store'};
export async function GET(_request:Request,context:Context){try{
 const {holdingSlug,id}=await context.params,a=await requireWorkContext(holdingSlug);
 return Response.json({data:{graph:await projectGraph(a,id),handoffs:await projectHandoffs(a,id)}},{headers});
}catch(error){return errorResponse(error);}}
export async function POST(request:Request,context:Context){try{
 if(request.headers.get('origin')&&request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
 const key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
 const {holdingSlug,id}=await context.params,a=await requireWorkContext(holdingSlug);
 return Response.json({data:await projectStructureCommand(a,id,await request.json(),key)},{headers});
}catch(error){return errorResponse(error);}}
