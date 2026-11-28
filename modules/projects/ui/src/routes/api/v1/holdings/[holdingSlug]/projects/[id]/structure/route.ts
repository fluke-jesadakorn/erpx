import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {requestPublicOrigin} from "@erpx/manage-erpx-server/public/public-origin";
import {ApiError,errorResponse} from "@erpx/platform-runtime-server/public/http";
import {nativeProjectsPageContext} from "../../../../../../../../public/native-project-wbs-page";
import {nativeProjectStructureCommand} from "../../../../../../../../public/native-project-structure";
type Context={params:Promise<{holdingSlug:string;id:string}>};
const headers={'Cache-Control':'private, no-store'};
export async function POST(request:Request,context:Context){try{
 const origin=request.headers.get('origin');
 if(!origin||origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
 const key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
 const {holdingSlug,id}=await context.params,a=await requireWorkContext(holdingSlug);
 const data=await nativeProjectStructureCommand(request,nativeProjectsPageContext(a),holdingSlug,id,key,await request.json());
 return Response.json(canonicalDestinationLinks({data}),{headers});
}catch(error){return errorResponse(error);}}
